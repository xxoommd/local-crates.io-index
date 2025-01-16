use actix_files::Files;
use actix_web::{App, HttpServer};
use chrono::Local;
use git2::{FetchOptions, RemoteCallbacks, Repository};
use serde::Deserialize;
use std::{path::Path, sync::Arc, time::Duration};
use tokio::{time, signal};

#[derive(Debug, Deserialize)]
struct Config {
    repo: CratesIoIndexRepo,
    web: WebConfig,
}

#[derive(Debug, Deserialize)]
struct CratesIoIndexRepo {
    git_url: String,
    path: String,
    update_interval: u64,
}

#[derive(Debug, Deserialize)]
struct WebConfig {
    address: String,
    port: u16,
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // 读取配置文件
    let config_str = std::fs::read_to_string("config.toml").expect("Failed to read config.toml");
    let config: Config = toml::from_str(&config_str).expect("Failed to parse config.toml");

    // 初始化或更新git仓库
    let repo_path = Path::new(&config.repo.path);
    // 如果目录存在，直接使用
    if repo_path.exists() {
        println!("[{}] Using existing directory at {:?}", Local::now().format("%Y-%m-%d %H:%M:%S"), repo_path);
    } else {
        println!("[{}] Cloning repository...", Local::now().format("%Y-%m-%d %H:%M:%S"));
        clone_repo(&config.repo.git_url, repo_path);
    }

    // 启动定时pull任务
    let address = config.web.address.clone();
    let port = config.web.port;
    println!("[{}] Starting web server on {}:{}", Local::now().format("%Y-%m-%d %H:%M:%S"), address, port);

    // 启动web服务
    let static_path = Arc::new(config.repo.path.clone());
    let static_path_clone = Arc::clone(&static_path);

    // 启动定时pull任务
    let git_url = config.repo.git_url.clone();
    let repo_path = config.repo.path.clone();
    let update_interval = config.repo.update_interval;
    tokio::spawn(async move {
        let mut interval = time::interval(Duration::from_secs(update_interval)); // 每小时pull一次
        loop {
            interval.tick().await;
            println!("[{}] Pulling repository updates...", Local::now().format("%Y-%m-%d %H:%M:%S"));
            if let Ok(repo) = Repository::open(&repo_path) {
                pull_repo(&repo, &git_url);
            }
        }
    });

    let server = HttpServer::new(move || {
        App::new().service(Files::new("/", &*static_path_clone).show_files_listing())
    })
    .workers(8)
    .bind((address.clone(), port))?;

    println!("[{}] Web server started at http://{}:{}", Local::now().format("%Y-%m-%d %H:%M:%S"), address, port);

    let server = server.run();
    tokio::select! {
        result = server => {
            if let Err(e) = result {
                println!("[{}] 服务器异常关闭: {}", Local::now().format("%Y-%m-%d %H:%M:%S"), e);
            } else {
                println!("[{}] 服务器正常关闭", Local::now().format("%Y-%m-%d %H:%M:%S"));
            }
        }
        _ = signal::ctrl_c() => {
            println!("[{}] 收到终止信号，正在优雅关闭 web server...", Local::now().format("%Y-%m-%d %H:%M:%S"));
        }
    }

    println!("[{}] Web server优雅关闭完成", Local::now().format("%Y-%m-%d %H:%M:%S"));

    Ok(())
}

fn clone_repo(url: &str, path: &Path) -> Repository {
    let mut callbacks = RemoteCallbacks::new();
    callbacks.credentials(|_url, username_from_url, allowed_types| {
        if allowed_types.contains(git2::CredentialType::SSH_KEY) {
            let home_dir = dirs::home_dir().expect("Failed to get home directory");
            let private_key = home_dir.join(".ssh").join("id_rsa");
            let public_key = home_dir.join(".ssh").join("id_rsa.pub");
            git2::Cred::ssh_key(
                username_from_url.unwrap_or("git"),
                Some(&public_key),
                &private_key,
                None,
            )
        } else {
            git2::Cred::default()
        }
    });

    let mut fetch_options = FetchOptions::new();
    fetch_options.remote_callbacks(callbacks);

    let mut builder = git2::build::RepoBuilder::new();
    builder.fetch_options(fetch_options);

    builder
        .clone(url, path)
        .expect("Failed to clone repository")
}

fn pull_repo(repo: &Repository, url: &str) {
    let mut callbacks = RemoteCallbacks::new();
    callbacks.credentials(|_url, _username_from_url, _allowed_types| git2::Cred::default());

    let mut fetch_options = FetchOptions::new();
    fetch_options.remote_callbacks(callbacks);

    let mut remote = repo
        .find_remote("origin")
        .unwrap_or_else(|_| repo.remote("origin", url).expect("Failed to create remote"));

    remote
        .fetch(
            &["refs/heads/*:refs/heads/*"],
            Some(&mut fetch_options),
            None,
        )
        .expect("Failed to fetch");

    let fetch_head = repo
        .find_reference("FETCH_HEAD")
        .expect("Failed to find FETCH_HEAD");
    let fetch_commit = repo
        .reference_to_annotated_commit(&fetch_head)
        .expect("Failed to get commit from FETCH_HEAD");

    let analysis = repo
        .merge_analysis(&[&fetch_commit])
        .expect("Failed to analyze merge");

    if analysis.0.is_up_to_date() {
        println!("[{}] [{}] Already up-to-date", Local::now().format("%Y-%m-%d %H:%M:%S"), url);
    } else if analysis.0.is_fast_forward() {
        println!("[{}] [{}] Performing fast-forward merge", Local::now().format("%Y-%m-%d %H:%M:%S"), url);
        let mut reference = repo
            .find_reference("refs/heads/master")
            .expect("Failed to find master branch");
        reference
            .set_target(fetch_commit.id(), "Fast-forward")
            .expect("Failed to fast-forward");
        repo.set_head("refs/heads/master")
            .expect("Failed to set HEAD");
        repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))
            .expect("Failed to checkout HEAD");
    } else {
        println!("[{}] [{}] Merge required but not implemented", Local::now().format("%Y-%m-%d %H:%M:%S"), url);
    }
}
