use rayon::prelude::*;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::Semaphore;

use cargo_toml::Manifest;
use crates_index::{Crate, Version};
use flate2::read::GzDecoder;
use tar::Archive;
use tempdir::TempDir;

mod binstall;

pub struct BinstallMetrics {
    pub has_binstall_metadata: bool,
    pub uses_https: bool,
}

impl BinstallMetrics {
    pub fn new(manifest_path: &str) -> Self {
        if let Ok(manifest) = Manifest::<binstall::Meta>::from_path_with_metadata(manifest_path) {
            if let Some(metadata) = manifest.package.unwrap().metadata {
                Self {
                    has_binstall_metadata: metadata.binstall.is_some(),
                    uses_https: metadata
                        .binstall
                        .map(|e| e.pkg_url.starts_with("https://"))
                        .unwrap_or(false),
                }
            } else {
                Self {
                    has_binstall_metadata: false,
                    uses_https: false,
                }
            }
        } else {
            Self {
                has_binstall_metadata: false,
                uses_https: false,
            }
        }
    }
}

pub struct Metric {
    pub crate_: Crate,
    pub version: Version,
    pub binstall: BinstallMetrics,
}

#[tokio::main]
async fn main() {
    let mut index = crates_index::Index::new_cargo_default().unwrap();
    println!("Updating index");
    index.update().unwrap();
    let mut crates_with_binstall_support = HashMap::<String, usize>::new();
    let mut versions_not_supporting_https = HashMap::<(String, String), String>::new();

    println!("Counting index");
    let creates_count = index.crates().count();

    let config = index.index_config().unwrap();

    let data = index
        .crates()
        .enumerate()
        .map(|(c, single_crate)| {
            let config = config.clone();
            async move {
                if c % 500 == 0 {
                    println!(
                        "crate name: {}   {}/{}\t\t",
                        single_crate.name(),
                        c,
                        creates_count
                    );
                }
                let futs = single_crate
                    .versions()
                    .par_iter()
                    .filter_map(|version| {
                        if version.is_yanked() {
                            return None;
                        }

                        let temp_dir = TempDir::new("cargo-binstall").unwrap();

                        let temp_dir_path = temp_dir.path().to_owned();

                        let crate_url = version.download_url(&config).unwrap();
                        let tgz_path = temp_dir_path
                            .clone()
                            .join(format!("{}.tgz", version.name()));

                        Some(async move {
                            let url = &crate_url.clone();
                            let resp = backoff::future::retry(
                                backoff::ExponentialBackoff::default(),
                                || async move { Ok(reqwest::get(url).await?) },
                            )
                            .await
                            .unwrap();

                            if !resp.status().is_success() {
                                return None;
                            }

                            let bytes = resp.bytes().await.unwrap();

                            std::fs::create_dir_all(tgz_path.parent().unwrap()).unwrap();
                            std::fs::write(&tgz_path, bytes).unwrap();

                            let dat = std::fs::File::open(&tgz_path).unwrap();
                            let tar = GzDecoder::new(dat);
                            let mut tgz = Archive::new(tar);

                            tgz.unpack(temp_dir_path.clone()).unwrap();

                            let crate_path = temp_dir_path.clone().join(format!(
                                "{}-{}",
                                version.name(),
                                version.version()
                            ));

                            let metrics = BinstallMetrics::new(
                                crate_path.join("Cargo.toml").to_str().unwrap(),
                            );

                            temp_dir.close().unwrap();

                            Some((metrics, version))
                        })
                    })
                    .collect::<Vec<_>>();

                let mut metrics = vec![];

                for fut in futs {
                    metrics.push(fut.await);
                }

                (
                    (
                        single_crate.name().to_string(),
                        metrics
                            .par_iter()
                            .filter_map(|e: &Option<(BinstallMetrics, &Version)>| {
                                if let Some(v) = e {
                                    if v.0.has_binstall_metadata {
                                        Some(v.0.has_binstall_metadata)
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            })
                            .count(),
                    ),
                    metrics
                        .par_iter()
                        .filter_map(|metric| {
                            if let Some(m) = metric {
                                if m.0.has_binstall_metadata && !m.0.uses_https {
                                    Some((
                                        (
                                            single_crate.to_owned().name().to_string(),
                                            m.1.to_owned().version().to_string(),
                                        ),
                                        m.1.download_url(&config).unwrap(),
                                    ))
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>(),
                )
            }
        })
        .collect::<Vec<_>>();

    let mut futs = vec![];
    let semaphore = Arc::new(Semaphore::new(num_cpus::get() * 4));

    for item in data {
        let permit = semaphore.clone().acquire_owned().await;
        futs.push(tokio::spawn(async move {
            let data = item.await;
            drop(permit);
            data
        }));
    }

    for fin in futs {
        if let Ok((install_meta_data, https_use)) = fin.await {
            crates_with_binstall_support.insert(install_meta_data.0, install_meta_data.1);
            if install_meta_data.1 > 0 {
                println!("+1");
            }
            for (key, value) in https_use {
                versions_not_supporting_https.insert(key, value);
            }
        }
    }

    println!("{:?}", crates_with_binstall_support.keys());
    println!("{:?}", versions_not_supporting_https);
}
