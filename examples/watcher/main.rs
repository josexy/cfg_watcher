use cfg_watcher::{CancellationToken, ConfigWatcher, read_config};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
struct AppConfig {
    address: String,
    port: usize,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config_path = "config.json";

    let token = CancellationToken::new();
    let token_clone = token.clone();
    let mut cfg = ConfigWatcher::<AppConfig>::new(&config_path, token_clone)?;
    let c: AppConfig = read_config(&config_path)?;
    println!("init config: {:#?}", c);

    loop {
        tokio::select! {
            v = cfg.recv() => {
                if v.is_none() {
                    break
                }
                let ev = v.unwrap();
                println!("old: {:#?}, new: {:#?}", ev.0, ev.1);
            }
            _ = token.cancelled() => {
                break;
            }
        }
    }

    token.cancel();
    Ok(())
}
