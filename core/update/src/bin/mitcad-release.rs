// SPDX-License-Identifier: MIT
//! `mitcad-release`: signs a release's downloads and writes its update
//! manifest (`docs/updates.md`). The private key never comes on the command
//! line: it is read from the environment at release time.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::Path;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use mitcad_update::{
    MANIFEST_FORMAT, Manifest, TrustedKeys, assess, parse_version, public_key, sign_file,
    signing_key,
};

const USAGE: &str = "\
Usage:
  mitcad-release public-key
      Prints the public key of the release key (to build into Mitcad).
  mitcad-release manifest --version VERSION --notes URL [--date YYYY-MM-DD]
                          [--asset PLATFORM FILE URL]... --out FILE
      Signs each file as the download for its platform (windows-x64,
      linux-x64) at URL and writes the signed manifest. The date defaults
      to today (UTC).
  mitcad-release verify --manifest FILE --key PUBLIC-KEY [--asset PLATFORM FILE]...
      Checks a manifest and files against a public key.

The release key (64 hex digits, the Ed25519 seed; for example from
`openssl rand -hex 32`) is MITCAD_RELEASE_KEY, or the content of the file
MITCAD_RELEASE_KEY_FILE names.";

enum Failure {
    Usage(String),
    Error(String),
}

fn error(message: impl ToString) -> Failure {
    Failure::Error(message.to_string())
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failure::Usage(message)) => {
            eprintln!("mitcad-release: {message}\n\n{USAGE}");
            ExitCode::from(2)
        }
        Err(Failure::Error(message)) => {
            eprintln!("mitcad-release: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<(), Failure> {
    let Some((command, rest)) = args.split_first() else {
        return Err(Failure::Usage("no command".into()));
    };
    match command.as_str() {
        "public-key" if rest.is_empty() => {
            println!("{}", public_key(&release_key()?));
            Ok(())
        }
        "manifest" => manifest(rest),
        "verify" => verify(rest),
        "help" | "--help" | "-h" => {
            println!("{USAGE}");
            Ok(())
        }
        _ => Err(Failure::Usage(format!(
            "unknown command '{}'",
            args.join(" ")
        ))),
    }
}

/// The private key from the environment.
fn release_key() -> Result<ed25519_dalek::SigningKey, Failure> {
    let text = if let Some(key) = env::var_os("MITCAD_RELEASE_KEY").filter(|v| !v.is_empty()) {
        key.into_string()
            .map_err(|_| error("MITCAD_RELEASE_KEY is not text"))?
    } else if let Some(path) = env::var_os("MITCAD_RELEASE_KEY_FILE").filter(|v| !v.is_empty()) {
        fs::read_to_string(&path).map_err(|e| {
            error(format!(
                "cannot read the key file {}: {e}",
                Path::new(&path).display()
            ))
        })?
    } else {
        return Err(error(
            "no release key: set MITCAD_RELEASE_KEY or MITCAD_RELEASE_KEY_FILE",
        ));
    };
    signing_key(&text).map_err(error)
}

/// The options: `--name value...` with as many values as `arity` says.
fn options<'a>(
    args: &'a [String],
    arity: &[(&str, usize)],
) -> Result<Vec<(&'a str, &'a [String])>, Failure> {
    let mut found = Vec::new();
    let mut rest = args;
    while let Some((name, tail)) = rest.split_first() {
        let Some(&(_, count)) = arity.iter().find(|(option, _)| option == name) else {
            return Err(Failure::Usage(format!("unknown option '{name}'")));
        };
        if tail.len() < count {
            return Err(Failure::Usage(format!("{name} needs {count} value(s)")));
        }
        found.push((name.as_str(), &tail[..count]));
        rest = &tail[count..];
    }
    Ok(found)
}

fn single<'a>(found: &[(&str, &'a [String])], name: &str) -> Option<&'a str> {
    found
        .iter()
        .rev()
        .find(|(option, _)| *option == name)
        .map(|(_, values)| values[0].as_str())
}

fn manifest(args: &[String]) -> Result<(), Failure> {
    let found = options(
        args,
        &[
            ("--version", 1),
            ("--notes", 1),
            ("--date", 1),
            ("--asset", 3),
            ("--out", 1),
        ],
    )?;
    let required = |name: &str| {
        single(&found, name).ok_or_else(|| Failure::Usage(format!("{name} is required")))
    };
    let version = parse_version(required("--version")?)
        .map_err(error)?
        .to_string();
    let notes = required("--notes")?;
    let out = required("--out")?;
    let date = match single(&found, "--date") {
        Some(date) => date.to_string(),
        None => today(),
    };
    let key = release_key()?;
    let mut assets = BTreeMap::new();
    for (_, values) in found.iter().filter(|(option, _)| *option == "--asset") {
        let (platform, file, url) = (&values[0], Path::new(&values[1]), &values[2]);
        let asset = sign_file(&version, platform, file, url, &key).map_err(error)?;
        println!(
            "{platform}: {} ({} bytes, SHA-256 {})",
            file.display(),
            asset.size,
            asset.sha256
        );
        assets.insert(platform.clone(), asset);
    }
    let manifest = Manifest {
        format: MANIFEST_FORMAT,
        version,
        date,
        notes: notes.to_string(),
        assets,
    };
    let signed = manifest.signed(&key);
    // Signed is not enough: what the application reads must pass its checks.
    Manifest::read(
        signed.as_bytes(),
        &TrustedKeys::parse(&public_key(&key)).map_err(error)?,
    )
    .map_err(error)?;
    fs::write(out, signed).map_err(|e| error(format!("cannot write {out}: {e}")))?;
    println!(
        "{out}: Mitcad {} signed with {}",
        manifest.version,
        public_key(&key)
    );
    Ok(())
}

fn verify(args: &[String]) -> Result<(), Failure> {
    let found = options(
        args,
        &[
            ("--manifest", 1),
            ("--key", 1),
            ("--asset", 2),
            ("--current", 1),
        ],
    )?;
    let path = single(&found, "--manifest")
        .ok_or_else(|| Failure::Usage("--manifest is required".into()))?;
    let keys = TrustedKeys::parse(
        single(&found, "--key").ok_or_else(|| Failure::Usage("--key is required".into()))?,
    )
    .map_err(error)?;
    let data = fs::read(path).map_err(|e| error(format!("cannot read {path}: {e}")))?;
    let manifest = Manifest::read(&data, &keys).map_err(error)?;
    println!(
        "{path}: Mitcad {} of {}, signature good",
        manifest.version, manifest.date
    );
    for (_, values) in found.iter().filter(|(option, _)| *option == "--asset") {
        manifest
            .verify_file(&values[0], Path::new(&values[1]), &keys)
            .map_err(|e| error(format!("{}: {e}", values[1])))?;
        println!("{}: {} download good", values[1], values[0]);
    }
    if let Some(current) = single(&found, "--current") {
        let status =
            assess(&manifest, current, mitcad_update::Channel::Stable, "").map_err(error)?;
        println!("for {current}: {status:?}");
    }
    Ok(())
}

/// Today's date (UTC), `YYYY-MM-DD`.
fn today() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
    let days = (seconds / 86_400) as i64 + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}
