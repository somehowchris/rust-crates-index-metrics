#[macro_use]
extern crate tracing;

use rayon::prelude::*;
use std::borrow::Cow;

use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio_util::io::StreamReader;

use indicatif::{ProgressBar, ProgressStyle};
use std::sync::RwLock;

use async_std::path::Path;
use async_tar::Archive;
use cargo_toml::Manifest;
use crates_index::{Crate, Version};
use futures::io::AsyncReadExt;
use futures::TryStreamExt;
use futures_util::StreamExt;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod binstall;

pub struct BinstallMetrics {
    pub has_binstall_metadata: bool,
    pub uses_https: bool,
}

impl BinstallMetrics {
    pub fn new(manifest_path: Vec<u8>) -> Self {
        if let Ok(manifest) = Manifest::<binstall::Meta>::from_slice_with_metadata(&manifest_path) {
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
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let mut index = crates_index::Index::new_cargo_default().unwrap();

    info!("Updating index");
    index.update().unwrap();

    let config = index.index_config().unwrap();

    let versions = index
        .crates_parallel()
        .filter_map(|e| e.ok())
        .flat_map(|e| {
            e.versions()
                .iter()
                .filter(|v| !v.is_yanked())
                .cloned()
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    let total_versions = versions.len();

    let data = versions.into_iter().map(|version| {
        let config = config.clone();

        let crate_url = version.download_url(&config).unwrap();

        async move {
            let url = &crate_url.clone();
            let resp =
                backoff::future::retry(backoff::ExponentialBackoff::default(), || async move {
                    Ok(reqwest::get(url).await?)
                })
                .await
                .unwrap();

            if !resp.status().is_success() {
                return None;
            }

            let tgz = Archive::new(
                tokio_util::io::ReaderStream::new(
                    async_compression::tokio::bufread::GzipDecoder::new(StreamReader::new(
                        resp.bytes_stream()
                            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e)),
                    )),
                )
                .into_async_read(),
            );

            let mut entries = tgz.entries().unwrap();

            let crate_file = format!("{}-{}/Cargo.toml", version.name(), version.version());

            let path = Cow::from(Path::new(&crate_file));

            let mut buff = None;

            while let Some(file) = entries.next().await {
                if let Ok(mut value) = file {
                    if let Ok(file_path) = value.path() {
                        if path == file_path {
                            let mut buffer = vec![];

                            value.read_to_end(&mut buffer).await.unwrap();
                            buff = Some(buffer);

                            break;
                        }
                    }
                }
            }

            if let Some(buffer) = buff {
                let metrics = BinstallMetrics::new(buffer);

                Some((metrics, version))
            } else {
                None
            }
        }
    });

    let semaphore = Arc::new(Semaphore::new(num_cpus::get() * 8));
    let bar = Arc::new(ProgressBar::new(total_versions.try_into().unwrap()));

    bar.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg}")
            .progress_chars("##-"),
    );

    let results = Arc::new(RwLock::new(
        Vec::<(BinstallMetrics, Version)>::with_capacity(total_versions),
    ));

    for item in data {
        let permit = semaphore.clone().acquire_owned().await;
        let progress_bar = Arc::clone(&bar);
        let results = Arc::clone(&results);
        tokio::spawn(async move {
            let data = item.await;

            progress_bar.inc(1);
            drop(permit);
            if let Some((metrics, version)) = data {
                let mut result = results.write().unwrap();
                result.push((metrics, version));
            }
        });
    }

    let data = results.read().unwrap();

    let binstall_data = data
        .par_iter()
        .filter(|e| e.0.has_binstall_metadata)
        .collect::<Vec<_>>();

    info!("versions with binstall support: {:?}", binstall_data.len());
    info!(
        "crates with binstall support: {:?}",
        binstall_data
            .par_iter()
            .map(|(_metric, version)| version.name())
            .collect::<HashSet<_>>()
            .len()
    );
    info!(
        "names of crates with binstall support: {:?}",
        binstall_data
            .par_iter()
            .map(|(_metric, version)| version.name())
            .collect::<HashSet<_>>()
    );
}
