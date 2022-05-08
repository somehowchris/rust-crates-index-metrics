# crates index metrics

This project is intendet to collect DX as well as Ops data for crates on an index (doesn't have to be `crates.io`).
> It's intention is to monitor an index for ease of use, security aspects, changes in dependencies and much more. Not using artificial tooling but rather gathering data based on real tools and applications.

## Status

This is currently in planning, a small poc based on collecting data about `cargo-binstall` is currently available inside this repository.

What it includes:
 - Creating a map of all crates and versions of an index
 - Downloading the crates file and parsing it's `Cargo.toml`
 - Using streaming and semaphores to reduce the amount needed for compute, ram and to adapt for greater local ressources
 - Gather raw data and collect key metrics based on this. > This currently means, gathering data and calulating the amount and names of crates supporting `cargo-binstall`

## The greater plan

What currenty is wrong:
 - As of right now, all data is downloaded as you go, this means reevaulating means a new download => files should be cachable
 - Errors are just spit out the console, there is no gathering about the process or plain supervision
 - Everything (except downloading the crates file) is a one shot operation

What it should provide in structure:
- It should adhere the follwing tree strucutring
    - Crate -> the crates name
        - Version -> the crates version
            - Version checksum -> the crates checksum
                - Feature -> hashes of `Vec<String>` as its storted by `asc alph, asc len`
                    - Target -> the platform targetet
                        - Package -> bins or libs of crate

What should trigger a new check:
 - A new crate
 - A new check
 - A new target
 - An updated checksum of a version
 - Updated dependencies -> From a lock file perspective, not one of a version specification in `Cargo.toml`

What should be checked:
 - tools
    - cargo udeps
    - cargo geiger
    - cargo tarpaulin
    - cargo build
    - cargo audit
    - cargo-bloat
  - questions:
   - Does it just build?
   - How long does it build?
   - How big are builds?
   - How many dependencies does it have?
   - How many features does it have?
   - Does it have any runtime dependencies?
   - Is its source code public? And still accessible
   - Are all dependencies up to date?
   - Are some dependencies using yanked crate versions?
   - Does it support cargo-binstall?
   - Is the maintainers email address valid?
   - How many binaries does it have?
   - does it use openssl?
   - On which host is the source code?
   - Does the cli use clap?
   - Is it wasm capable?
   - How much of its code is unsafe? Is unsafe code forbidden
   - How many test cases does it have?
   - What benching framework does it use?
   - Do all feature combinations work?
   - Does it use `rust 2021`
   - Does it use `dependabot` or `renovate`?
   - Is there more than 1 maintainer?
   - Are dependency versions clearly defined?
   - Does it support `no_std`?
   - What features do use up the most binary size?
   - Are binaries optimized?

