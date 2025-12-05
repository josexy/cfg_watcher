use std::{
    path::{self, Path, PathBuf},
    time::Duration,
};

use config::{Config, File, FileFormat, FileSource, FileSourceFile};
use notify_debouncer_full::{DebounceEventResult, new_debouncer, notify::RecursiveMode};
use serde_core::Deserialize;
use tokio::sync;
use tokio_util::sync::CancellationToken;

use crate::Error;

pub struct UpdateEvent<T>(pub T, pub T);

pub struct ConfigWatcher<T> {
    event_rx: sync::mpsc::UnboundedReceiver<UpdateEvent<T>>,
}

impl<T> ConfigWatcher<T>
where
    T: Deserialize<'static> + Send + Clone + 'static,
{
    pub fn new<P: AsRef<Path>>(path: P, cancel: CancellationToken) -> Result<Self, Error> {
        let path: PathBuf = FileSourceFile::new(path.as_ref().to_path_buf())
            .resolve(None::<FileFormat>)
            .map_err(|e| Error::ParseError(format!("failed to resolve {e}")))?
            .uri()
            .clone()
            .unwrap_or_default()
            .into();

        let cfg = read_config(&path)?;
        let (event_tx, event_rx) = sync::mpsc::unbounded_channel();

        log::trace!("found configuration file `{}`", path.display());

        tokio::spawn(watch_config(path, cancel, event_tx, cfg));
        Ok(Self { event_rx })
    }

    pub async fn recv(&mut self) -> Option<UpdateEvent<T>> {
        self.event_rx.recv().await
    }
}

pub fn read_config<P, T>(path: P) -> Result<T, Error>
where
    P: AsRef<Path>,
    T: Deserialize<'static> + Send + Clone + 'static,
{
    let path = path.as_ref();
    Config::builder()
        .add_source(File::with_name(&path.to_string_lossy()))
        .build()
        .map_err(|e| Error::ParseError(e.to_string()))?
        .try_deserialize::<T>()
        .map_err(|e| {
            Error::ParseError(format!(
                "failed to deserialize {}, error: {}",
                path.display(),
                e
            ))
        })
}

async fn watch_config<T>(
    mut config_file: PathBuf,
    cancel: CancellationToken,
    event_tx: sync::mpsc::UnboundedSender<UpdateEvent<T>>,
    mut last_config: T,
) -> Result<(), Error>
where
    T: Deserialize<'static> + Send + Clone + 'static,
{
    let (tx, mut rx) = sync::mpsc::unbounded_channel();

    config_file = path::absolute(config_file)?;

    let config_dir = config_file.parent().ok_or(Error::ParseError(format!(
        "failed to get parent dir {}",
        config_file.display()
    )))?;

    let mut real_config_file = std::fs::canonicalize(&config_file)?;
    let config_file_clone = config_file.clone();

    let mut debouncer = new_debouncer(
        Duration::from_millis(400),
        None,
        move |result: DebounceEventResult| match result {
            Ok(events) => events.iter().for_each(|event| {
                event.paths.iter().for_each(|e| {
                    if let Ok(current_config_file) = std::fs::canonicalize(&config_file_clone) {
                        // the same action with Go Viper
                        // we only care about the config file with the following cases:
                        // 1 - if the config file was modified or created
                        // 2 - if the real path to the config file changed (eg: k8s ConfigMap replacement)
                        if (e == &config_file_clone
                            && (event.kind.is_modify() || event.kind.is_create()))
                            || (!current_config_file.as_os_str().is_empty()
                                && current_config_file != real_config_file)
                        {
                            log::trace!(
                                "watch event {:?}-{}, raw: {}, last: {}, current: {}",
                                event.kind,
                                e.display(),
                                config_file_clone.display(),
                                real_config_file.display(),
                                current_config_file.display(),
                            );
                            real_config_file = current_config_file;

                            let _ = tx.send(());
                        }
                    } else if event.kind.is_remove() {
                        log::error!("config file {} was removed", config_file_clone.display());
                    }
                });
            }),
            Err(errors) => errors.iter().for_each(|error| {
                log::error!("debouncer error: {}", error);
            }),
        },
    )
    .map_err(|e| Error::ParseError(e.to_string()))?;
    debouncer
        .watch(config_dir, RecursiveMode::NonRecursive)
        .map_err(|e| Error::ParseError(e.to_string()))?;
    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                break
            }
            e = rx.recv() => {
                match e {
                    Some(_) => {
                        match read_config::<&PathBuf, T>(&config_file){
                            Ok(cfg) => {
                                let _ = event_tx.send(UpdateEvent(last_config.clone(), cfg.clone()));
                                last_config = cfg;
                            }
                            Err(e) => {
                                log::error!("failed to read config: {}", e);
                            }
                        }
                    }
                    None => {
                        break
                    }
                }
            }
        }
    }
    debouncer.stop_nonblocking();
    Ok(())
}
