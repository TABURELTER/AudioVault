//! Single-instance enforcement.
//!
//! The primary process holds an exclusive OS file lock on `instance.lock` for
//! its whole lifetime (released automatically by the OS if it crashes) and
//! listens on a loopback port published in `instance.port` together with a
//! random token. A second process fails to take the lock, asks the primary to
//! bring its window to front, and exits.

use std::fs::{File, OpenOptions, TryLockError};
use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::Result;

pub struct SingleInstance {
    _lock: File,
    listener: Option<TcpListener>,
    token: String,
    port_file: PathBuf,
}

pub enum InstanceRole {
    Primary(SingleInstance),
    /// Another instance owns the catalog. `notified` tells whether it was asked to activate.
    Secondary {
        notified: bool,
    },
}

fn port_file_for(lock_path: &Path) -> PathBuf {
    lock_path.with_extension("port")
}

impl SingleInstance {
    pub fn acquire(lock_path: &Path) -> Result<InstanceRole> {
        if let Some(parent) = lock_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock_path)?;
        match file.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => {
                let notified = notify_primary(&port_file_for(lock_path));
                return Ok(InstanceRole::Secondary { notified });
            }
            Err(TryLockError::Error(e)) => return Err(e.into()),
        }
        let token = uuid::Uuid::new_v4().simple().to_string();
        let port_file = port_file_for(lock_path);
        let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).ok();
        if let Some(l) = &listener {
            let port = l.local_addr()?.port();
            std::fs::write(&port_file, format!("{port} {token}"))?;
        }
        Ok(InstanceRole::Primary(SingleInstance {
            _lock: file,
            listener,
            token,
            port_file,
        }))
    }

    /// Spawn a thread that invokes `on_activate` whenever another launch asks
    /// us to come to the foreground.
    pub fn listen(&mut self, on_activate: impl Fn() + Send + 'static) {
        let Some(listener) = self.listener.take() else {
            return;
        };
        let token = self.token.clone();
        let _ = std::thread::Builder::new()
            .name("single-instance".into())
            .spawn(move || {
                for stream in listener.incoming() {
                    let Ok(stream) = stream else { continue };
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                    let mut line = String::new();
                    if BufReader::new(stream).take_line(&mut line)
                        && line.trim() == format!("ACTIVATE {token}")
                    {
                        on_activate();
                    }
                }
            });
    }
}

impl Drop for SingleInstance {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.port_file);
    }
}

trait TakeLine {
    fn take_line(self, buf: &mut String) -> bool;
}

impl<R: std::io::Read> TakeLine for BufReader<R> {
    fn take_line(mut self, buf: &mut String) -> bool {
        use std::io::Read;
        let mut limited = String::new();
        let ok = (&mut self).take(256).read_line(&mut limited).is_ok();
        *buf = limited;
        ok
    }
}

fn notify_primary(port_file: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(port_file) else {
        return false;
    };
    let mut parts = text.split_whitespace();
    let (Some(port), Some(token)) = (
        parts.next().and_then(|p| p.parse::<u16>().ok()),
        parts.next(),
    ) else {
        return false;
    };
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    match TcpStream::connect_timeout(&addr, Duration::from_secs(2)) {
        Ok(mut s) => s
            .write_all(format!("ACTIVATE {token}\n").as_bytes())
            .is_ok(),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn second_instance_is_rejected_and_notifies() {
        let dir = tempfile::tempdir().unwrap();
        let lock = dir.path().join("instance.lock");
        let InstanceRole::Primary(mut primary) = SingleInstance::acquire(&lock).unwrap() else {
            panic!("first must be primary")
        };
        let hit = Arc::new(AtomicBool::new(false));
        let h = hit.clone();
        primary.listen(move || h.store(true, Ordering::SeqCst));
        match SingleInstance::acquire(&lock).unwrap() {
            InstanceRole::Secondary { notified } => assert!(notified),
            InstanceRole::Primary(_) => panic!("second must be secondary"),
        }
        for _ in 0..50 {
            if hit.load(Ordering::SeqCst) {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(hit.load(Ordering::SeqCst));
        drop(primary);
        assert!(matches!(
            SingleInstance::acquire(&lock).unwrap(),
            InstanceRole::Primary(_)
        ));
    }
}
