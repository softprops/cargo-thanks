extern crate clap;
extern crate env_logger;
#[macro_use]
extern crate log;
extern crate cargo_metadata;
extern crate hyper;
extern crate hyper_tls;
extern crate tokio_core;
extern crate futures;
extern crate serde_json;
#[macro_use]
extern crate error_chain;
#[macro_use]
extern crate serde_derive;
extern crate url;
extern crate url_serde;
extern crate yansi;
extern crate hubcaps;

use std::env;

use cargo_metadata::Error as CargoError;
use clap::App;
use futures::{Future, Stream};
use futures::stream::futures_unordered;
use hubcaps::{Credentials, Error as GithubError, Github};
use hyper::{Client, Error as HttpError};
use hyper_tls::HttpsConnector;
use serde_json::error::Error as SerdeError;
use std::collections::HashSet;
use std::io::Error as IoError;
use tokio_core::reactor::Core;
use url::Url;
use yansi::Paint;

error_chain! {
    foreign_links {
        Codec(SerdeError);
        Http(HttpError);
        IO(IoError);
        Cargo(CargoError);
        Github(GithubError);
    }
}

quick_main!(run);

fn run() -> Result<()> {
    drop(env_logger::init());
    // not actually parsing args for the moment
    App::new(env!("CARGO_PKG_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .about(
            "Thanks rust lang dependencies on github.com
            this program assumes a github token stored in a GITHUB_TOKEN env variable"
            )

        .get_matches();
    let mut core = Core::new()?;
    let github = match env::var("GITHUB_TOKEN") {
        Ok(token) => {
            Github::new(
                concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")),
                Some(Credentials::Token(token)),
                &core.handle(),
            )
        }
        _ => return Err("GITHUB_TOKEN is required".into()),
    };

    let metadata = cargo_metadata::metadata(None)?;
    let deps = metadata.packages.into_iter().fold(
        HashSet::new(),
        |mut acc, pkg| {
            for dep in pkg.dependencies {
                acc.insert(dep.name);
            }
            acc
        },
    );

    let http = Client::configure()
        .connector(HttpsConnector::new(4, &core.handle()).unwrap())
        .keep_alive(true)
        .build(&core.handle());

    let crates = deps.iter().map(|dep| {
        http.get(
            format!("https://crates.io/api/v1/crates/{dep}", dep = dep)
                .parse()
                .unwrap(),
        ).map_err(Error::from)
            .and_then(|response| {
                response.body().concat2().map_err(Error::from).and_then(
                    move |body| {
                        serde_json::from_slice::<Wrapper>(&body)
                            .map(|w| w.krate)
                            .map_err(Error::from)
                    },
                )
            })
    });
    let f = futures_unordered(crates)
        .filter_map(move |c| {
            c.repository
                .clone()
                .into_iter()
                .filter(move |repo| repo.host_str() == Some("github.com"))
                .next()
                .map(move |repo| {
                    (c.name, repo.path().trim_left_matches("/").to_owned())
                })
        })
        .for_each(|(name, repo)| {
            let r2 = repo.clone();
            let comps = r2.splitn(2, "/").collect::<Vec<_>>();
            debug!("starring {}/{}", comps[0], comps[1]);
            github
                .activity()
                .stars()
                .star(comps[0], comps[1])
                .inspect(move |_| {
                    println!(
                        "💖 {} {}",
                        name,
                        Paint::rgb(
                            128,
                            128,
                            128,
                            format!("github.com/{}", repo),
                        ).to_string()
                    );
                })
                .map_err(Error::from)
        });
    core.run(f)

}

#[derive(Debug, Deserialize)]
pub struct Wrapper {
    #[serde(rename = "crate")]
    krate: Crate,
}

#[derive(Debug, Deserialize)]
pub struct Crate {
    pub id: String,
    pub name: String,
    #[serde(deserialize_with = "url_serde::deserialize")]
    pub repository: Option<Url>,
}