extern crate clap;
extern crate env_logger;
#[macro_use]
extern crate log;
extern crate cargo_metadata;
extern crate futures;
extern crate hyper;
extern crate hyper_tls;
extern crate serde_json;
extern crate tokio_core;
#[macro_use]
extern crate error_chain;
#[macro_use]
extern crate serde_derive;
extern crate hubcaps;
extern crate url;
extern crate url_serde;
extern crate yansi;

use cargo_metadata::Error as CargoError;
use clap::{App, AppSettings, Arg, SubCommand};
use futures::stream::futures_unordered;
use futures::{Future, Stream};
use hubcaps::{Credentials, Error as GithubError, Github};
use hyper::{Client, Error as HttpError};
use hyper_tls::HttpsConnector;
use serde_json::error::Error as SerdeError;
use std::collections::HashSet;
use std::io::Error as IoError;
use std::result::Result as StdResult;
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

fn non_blank(arg: String) -> StdResult<(), String> {
    if arg.is_empty() {
        return Err("\n\n\tNo Github token was provided via --token or GITHUB_TOKEN env var".to_owned());
    }
    Ok(())
}

fn run() -> Result<()> {
    drop(env_logger::init());
    // not actually parsing args for the moment
    let matches = App::new("cargo-thanks")
        .setting(AppSettings::SubcommandRequired)
        .setting(AppSettings::DisableHelpSubcommand)
        .bin_name("cargo")
        .subcommand(
            SubCommand::with_name("thanks")
            .version(env!("CARGO_PKG_VERSION"))
            .author(env!("CARGO_PKG_AUTHORS"))
            .about(
                "Thanks rust lang dependencies on github.com \n\
                this program assumes a github token stored in a GITHUB_TOKEN env variable"
            )
            .arg(Arg::from_usage("-t, --token [TOKEN] 'The Github OAuth token to use'")
                .required(true)
                .env("GITHUB_TOKEN")
                .validator(non_blank))
    ).get_matches();
    let thanks_matches = matches.subcommand_matches("thanks").unwrap();
    let mut core = Core::new()?;
    let github = Github::new(
        concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")),
        Some(Credentials::Token(
            thanks_matches.value_of("token").unwrap().to_owned(),
        )),
        &core.handle(),
    );

    let metadata = cargo_metadata::metadata(None)?;
    let deps =
        metadata
            .packages
            .into_iter()
            .fold(HashSet::new(), |mut acc, pkg| {
                for dep in pkg.dependencies {
                    acc.insert(dep.name);
                }
                acc
            });

    let http = Client::builder()
        .keep_alive(true)
        .build::<_, hyper::Body>(HttpsConnector::new(4).unwrap());

    let crates = deps.iter().map(|dep| {
        http.get(
            format!("https://crates.io/api/v1/crates/{dep}", dep = dep)
                .parse()
                .unwrap(),
        )
        .map_err(Error::from)
        .and_then(|response| {
            response
                .into_body()
                .concat2()
                .map_err(Error::from)
                .and_then(move |body| {
                    serde_json::from_slice::<Wrapper>(&body)
                        .map(|w| w.krate)
                        .map_err(Error::from)
                })
        })
    });
    let f =
        futures_unordered(crates)
            .filter_map(move |c| {
                c.repository
                    .clone()
                    .into_iter()
                    .filter(move |repo| repo.host_str() == Some("github.com"))
                    .next()
                    .map(move |repo| {
                        debug!("{}", repo.path());
                        (c.name, repo.path().trim_matches('/').to_owned())
                    })
            })
            .for_each(|(krate, path)| {
                let (owner, repo) = repo_uri(path.clone());
                debug!("starring {}/{}", owner, repo);
                github.activity().stars().star(owner, repo).then(
                    move |result| match result {
                        Ok(v) => {
                            println!(
                                "💖 {} {}",
                                krate,
                                Paint::rgb(
                                    128,
                                    128,
                                    128,
                                    format!("github.com/{}", path.as_str()),
                                )
                                .to_string()
                            );
                            Ok(v)
                        }
                        Err(e) => {
                            println!(
                                "💔 {} {}",
                                krate,
                                Paint::rgb(128, 128, 128, format!("{}", e),)
                                    .to_string()
                            );
                            Err(e.into())
                        }
                    },
                )
            });
    core.run(f)
}

fn repo_uri<P>(path: P) -> (String, String)
where
    P: Into<String>,
{
    let clone = path.into().clone();
    let parts = clone.splitn(2, '/').collect::<Vec<_>>();
    (parts[0].into(), parts[1].trim_end_matches(".git").into())
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

#[cfg(test)]
mod tests {
    use super::repo_uri;
    #[test]
    fn repo_uri_handles_expected_case() {
        assert_eq!(repo_uri("foo/bar"), ("foo".into(), "bar".into()))
    }

    #[test]
    fn repo_uri_handles_git_ext() {
        assert_eq!(repo_uri("foo/bar.git"), ("foo".into(), "bar".into()))
    }
}
