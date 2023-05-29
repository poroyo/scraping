use anyhow::Error;
use rayon::prelude::*;
use reqwest::Client;
use scraping::parse::convert_file;
use scraping::scraping::{download_files, parse_json};
use std::io::Write;
use std::sync::{
    atomic::{AtomicU16, AtomicU8, Ordering},
    Arc, Mutex,
};

// const ALL_PAGES: usize = 132;

#[tokio::main]
async fn main() -> Result<(), Error> {
    // https://api.venusai.chat/characters?page=1
    let mut user_input = String::new();
    print!("Pages...: ");
    std::io::stdout().flush()?;
    std::io::stdin().read_line(&mut user_input)?;
    let all_pages = user_input.trim().parse::<usize>()?;

    let start = std::time::Instant::now();
    let vecs = Arc::new(Mutex::new(Vec::with_capacity(all_pages)));
    // let mut file_exe_vecs = Vec::with_capacity(ALL_CHARA);
    let pages = Arc::new(AtomicU8::new(1));

    let client = Client::new();
    let mut tasks = Vec::with_capacity(all_pages);

    for page in 1..=all_pages {
        let client = client.clone();
        let vecs = Arc::clone(&vecs);
        let pages = Arc::clone(&pages);
        tasks.push(tokio::spawn(async move {
            let url = format!("https://api.venusai.chat/characters?page={page}");
            let res = client.get(url).send().await.unwrap().text().await.unwrap();
            let data: serde_json::Value = serde_json::from_str(res.as_str()).unwrap();
            let a = parse_json(data).await.unwrap();
            vecs.lock().unwrap().push(a);
            println!("{pages:?}/{all_pages}");
            pages.fetch_add(1, Ordering::SeqCst);
        }));
    }

    for task in tasks {
        let _ = task.await?;
    }

    let result = vecs.lock().unwrap().concat();
    println!("{} file data colleted.", result.len());
    println!("{:?}", start.elapsed());
    println!("------");
    let start = std::time::Instant::now();
    std::fs::create_dir_all("new")?;

    let error = download_files(result).await?;

    println!("{:?}", start.elapsed());
    println!("{:?}", error.lock().unwrap());

    println!("------");

    let mut user_input = String::new();
    print!("Make character cards. Continue? (Y/n): ");
    std::io::stdout().flush()?;
    std::io::stdin().read_line(&mut user_input)?;
    let user_input = user_input.trim().to_uppercase();

    if user_input == "Y" {
        let start = std::time::Instant::now();
        let paths = std::fs::read_dir("./new/")?;
        std::fs::create_dir_all("./output/")?;
        std::fs::create_dir_all("./new/temp/")?;

        let error_vecs = Arc::new(Mutex::new(Vec::new()));
        let current_file_count = Arc::new(AtomicU16::new(1));
        let paths = paths.collect::<Vec<_>>();
        let all_chara = paths
            .iter()
            .filter_map(|x| x.as_ref().ok())
            .filter(|x| x.path().extension().is_some())
            .filter(|x| x.path().extension() != Some(std::ffi::OsStr::new("txt")))
            .count();
        // println!("{:?}", paths.len());

        paths.par_iter().for_each(|path| {
            if let Ok(path) = path {
                let path = path.path();
                let current_file_count = Arc::clone(&current_file_count);
                if let Some(ext) = path.extension() {
                    if ext.to_string_lossy() != "txt" {
                        match convert_file(&path) {
                            Ok(_) => {
                                println!("completed...{current_file_count:?}/{all_chara}");
                                current_file_count.fetch_add(1, Ordering::SeqCst);
                            }
                            Err(e) => {
                                println!("{path:?}: {e}");
                                error_vecs.lock().unwrap().push((path, e));
                                },
                        }
                    }
                }
            }
        });

        // convert_files(paths).await?;

        println!("{:?}", start.elapsed());
        println!("{:?}", error_vecs.lock().unwrap());
    } else {
        println!("Exiting...");
    }

    Ok(())
}
