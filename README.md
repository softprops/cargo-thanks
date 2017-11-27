# cargo thanks

Give thanks (in the form of [github stars](https://help.github.com/articles/about-stars/)) to your fellow Rustaceans

## Install

This is intended to be installed as a [cargo](http://doc.crates.io/index.html) plugin

```bash
$ cargo install cargo-thanks
```

### usage

Create a [github access token](https://github.com/settings/tokens) and
store its value in an env variable named `GITHUB_TOKEN`

Within any of your Cargo based Rust projects, run the following

```bash
$ cargo thanks
```

Doug Tangren (softprops) 2017