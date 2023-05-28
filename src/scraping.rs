use anyhow::Error;
use futures::stream::StreamExt;
use serde_json::Value;
use std::{
    path::Path,
    sync::{
        atomic::{AtomicU16, Ordering},
        Arc, Mutex,
    },
};
use tokio::fs;
use tokio::io::AsyncWriteExt;


const CONCURRENT_COUNT: usize = 100;

pub async fn parse_json(data: Value) -> Result<Vec<(String, String, String)>, Error> {
    let mut vecs = Vec::with_capacity(23);
    let data = data.get("data").unwrap().as_array().unwrap();
    for object in data {
        let object = object.as_object().unwrap();
        let id = object.get("id").unwrap().as_str().unwrap();
        let avatar = object.get("avatar").unwrap().as_str().unwrap();

        let text_url = format!("https://api.venusai.chat/characters/{id}");
        let image_url = format!("https://cdn.venusai.chat/images/bot-avatars/{avatar}");

        vecs.push((id.to_string(), text_url, image_url));
    }

    Ok(vecs)
}

async fn download_file(data: (String, String, String)) -> Result<(), Error> {
    let (id, text_url, image_url) = (data.0, data.1, data.2);
    let image_url_a = image_url.clone();
    let image_url_a = image_url_a.split('.').collect::<Vec<_>>();
    let image_url_ext = image_url_a[image_url_a.len() - 1];
    let text_res = reqwest::get(text_url).await?.text().await?;
    let image_res = reqwest::get(image_url).await?.bytes().await?;

    let text_path = format!("./new/{id}.txt");
    let img_path = format!("./new/{id}.{image_url_ext}");
    let text_path = Path::new(&text_path);
    let img_path = Path::new(&img_path);
    if !text_path.exists() {
        let mut text_file = fs::File::create(text_path).await?;
        text_file.write_all(text_res.as_bytes()).await?;
    } else {
        println!("{text_path:?} already exist.");
    }
    if !img_path.exists() {
        let mut image_file = fs::File::create(img_path).await?;
        image_file.write_all(image_res.as_ref()).await?;
    } else {
        println!("{img_path:?} already exist.");
    }

    Ok(())
}

pub async fn download_files(
    data: Vec<(String, String, String)>,
) -> Result<Arc<Mutex<Vec<Error>>>, Error> {
    let current_file_count = Arc::new(AtomicU16::new(1));
    let all_chara = data.len();
    let error_vecs = Arc::new(Mutex::new(Vec::new()));

    futures::stream::iter(data)
        .for_each_concurrent(CONCURRENT_COUNT, |item| {
            let item = item.clone();
            let current_file_count = Arc::clone(&current_file_count);
            let error_vecs = Arc::clone(&error_vecs);
            async move {
                let item1 = item.clone();
                match download_file(item).await {
                    Ok(_) => {
                        println!("completed...{current_file_count:?}/{all_chara}");
                        current_file_count.fetch_add(1, Ordering::SeqCst);
                    }
                    Err(e) => {
                        println!("{e}");
                        println!("retry to download...");
                        std::thread::sleep(std::time::Duration::from_millis(1000));
                        match download_file(item1).await {
                            Ok(_) => {
                                println!("retrying completed...{current_file_count:?}/{all_chara}");
                                current_file_count.fetch_add(1, Ordering::SeqCst);
                            }
                            Err(e) => {
                                println!("Download failed...{e}");
                                error_vecs.lock().unwrap().push(e);
                            }
                        }
                    }
                }
            }
        })
        .await;

    Ok(error_vecs)
}
