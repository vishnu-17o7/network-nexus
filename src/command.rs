use anyhow::{bail, Context, Result};
use std::{path::Path, process::Stdio, time::Duration};
use tokio::{io::AsyncReadExt, process::Command};

const OUTPUT_LIMIT: u64 = 2 * 1024 * 1024;

pub fn available(name: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| {
            std::env::split_paths(&p).any(|d| {
                let path = d.join(name);
                path.is_file() && executable(&path)
            })
        })
        .unwrap_or(false)
}
#[cfg(unix)]
fn executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .map(|m| m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}
#[cfg(not(unix))]
fn executable(_: &Path) -> bool {
    true
}

pub async fn run(program: &str, args: &[String], timeout: Duration) -> Result<String> {
    run_with_input(program, args, None, timeout).await
}

pub async fn run_with_input(
    program: &str,
    args: &[String],
    input: Option<Vec<u8>>,
    timeout: Duration,
) -> Result<String> {
    if !available(program) {
        bail!("{program} is unavailable. Install the corresponding optional system package.");
    }
    let mut child = Command::new(program)
        .args(args)
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("Could not start {program}"))?;
    let mut stdout = child.stdout.take().context("Missing stdout")?;
    let mut stderr = child.stderr.take().context("Missing stderr")?;
    let stdin = child.stdin.take();
    let work = async {
        let output = async {
            let mut v = Vec::new();
            (&mut stdout)
                .take(OUTPUT_LIMIT + 1)
                .read_to_end(&mut v)
                .await?;
            Ok::<_, anyhow::Error>(v)
        };
        let errors = async {
            let mut v = Vec::new();
            (&mut stderr)
                .take(OUTPUT_LIMIT + 1)
                .read_to_end(&mut v)
                .await?;
            Ok::<_, anyhow::Error>(v)
        };
        let write = async {
            if let (Some(mut stream), Some(data)) = (stdin, input) {
                use tokio::io::AsyncWriteExt;
                stream.write_all(&data).await?;
                stream.shutdown().await?;
            }
            Ok::<_, anyhow::Error>(())
        };
        let (out, err, (), status) = tokio::try_join!(output, errors, write, async {
            Ok::<_, anyhow::Error>(child.wait().await?)
        })?;
        if out.len() as u64 > OUTPUT_LIMIT || err.len() as u64 > OUTPUT_LIMIT {
            bail!("{program}: output exceeds 2 MiB limit");
        }
        if !status.success() {
            bail!(
                "{program}: {}",
                clean(&String::from_utf8_lossy(&err))
                    .trim()
                    .chars()
                    .take(500)
                    .collect::<String>()
            );
        }
        Ok(clean(&String::from_utf8_lossy(&out)))
    };
    match tokio::time::timeout(timeout, work).await {
        Ok(r) => r,
        Err(_) => bail!("{program} timed out after {} seconds", timeout.as_secs()),
    }
}
pub async fn cmd(program: &str, args: &[&str]) -> Result<String> {
    run(
        program,
        &args.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        Duration::from_secs(5),
    )
    .await
}

pub fn clean(value: &str) -> String {
    value
        .chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .collect()
}

pub fn split_nm(line: &str) -> Vec<String> {
    let mut fields = vec![String::new()];
    let mut escaped = false;
    for c in line.chars() {
        if escaped {
            fields.last_mut().unwrap().push(c);
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == ':' {
            fields.push(String::new());
        } else {
            fields.last_mut().unwrap().push(c);
        }
    }
    if escaped {
        fields.last_mut().unwrap().push('\\');
    }
    fields
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nm_fields_preserve_colons() {
        assert_eq!(
            split_nm("*:Cafe\\:5G:aa\\:bb:90"),
            vec!["*", "Cafe:5G", "aa:bb", "90"]
        );
    }
    #[tokio::test]
    async fn process_arguments_are_not_shell_code() {
        let output = cmd(
            "printf",
            &["%s", "$(touch /tmp/nexus-injection); echo hacked"],
        )
        .await
        .unwrap();
        assert_eq!(output, "$(touch /tmp/nexus-injection); echo hacked");
    }
    #[tokio::test]
    async fn timeout_is_bounded() {
        let r = run("sleep", &["2".into()], Duration::from_millis(40)).await;
        assert!(r.unwrap_err().to_string().contains("timed out"));
    }
}
