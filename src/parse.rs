use anyhow::{anyhow, Error};
use avif_decode::*;
use base64::{engine::general_purpose, Engine as _};
use bytes::{Buf, BufMut, BytesMut};
use image::io::Reader as ImageReader;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::PathBuf;
use rgb::ComponentMap;

#[derive(Debug)]
struct Chunk {
    chunk_type: String,
    chunk_data: Vec<u8>,
}

#[derive(Serialize, Deserialize, Debug)]
struct Character1 {
    name: String,
    #[serde(rename(serialize = "description"))]
    personality: String,
    #[serde(rename(serialize = "personality"))]
    description: String,
    #[serde(rename(serialize = "first_mes"))]
    first_message: String,
    #[serde(rename(serialize = "mes_example"))]
    example_dialogs: String,
    scenario: String,
}

fn read_chunks(data: &[u8]) -> Result<Vec<Chunk>, Error> {
    let mut vec_chunks = Vec::new();
    let mut buf = BytesMut::from(data);

    // Signature 읽기 (8 bytes)
    let signature = buf.split_to(8);

    // 시그니쳐 체크
    if signature.as_ref() != [137, 80, 78, 71, 13, 10, 26, 10] {
        return Err(anyhow!("Invalid PNG Signature"));
    }

    // Chunk들 읽기
    while buf.has_remaining() {
        // Length 읽기 (4 bytes)
        let length = buf.get_u32() as usize;

        // Chunk Type 읽기 (4 bytes)
        let chunk_type = buf.split_to(4);
        let chunk_type_str = String::from_utf8_lossy(&chunk_type);

        // Chunk Data 읽기 (Length bytes)
        let chunk_data = buf.split_to(length);

        // CRC 읽기 (4 bytes)
        let crc = buf.get_u32();

        // CRC 체크
        let mut haser = crc32fast::Hasher::new();
        haser.update(&chunk_type);
        haser.update(&chunk_data);
        if crc != haser.finalize() {
            return Err(anyhow!("CRC for {} is invalid", chunk_type_str));
        }

        let chunk = Chunk {
            chunk_type: chunk_type_str.into(),
            chunk_data: chunk_data.to_vec(),
        };
        vec_chunks.push(chunk);
    }

    Ok(vec_chunks)
}

fn parse_json_to_string(file: std::fs::File) -> Result<String, Error> {
    use std::io::BufReader;
    let reader = BufReader::new(file);
    let chara: Character1 = serde_json::from_reader(reader)?; // JSON 키 중에서 필요 없는 것 삭제
    let chara = serde_json::to_string(&chara)?; // 그리고 다시 문자열로 변환
    let mut buf = String::new();
    general_purpose::STANDARD.encode_string(chara.as_bytes(), &mut buf); // base64로 변환
    let text_chunk = format!("chara\0{buf}"); // 캐릭터 카드 형식임을 알려줌
    Ok(text_chunk)
}

fn add_text_chunck(data: Vec<u8>, text: String) -> Result<Vec<Chunk>, Error> {
    let mut chuncks = read_chunks(&data)? // 기존의 메타데이터 삭제
        .into_iter()
        .filter(|x| x.chunk_type != "tExt")
        .collect::<Vec<_>>();
    let text_chunck = Chunk {
        chunk_type: "tEXt".to_string(),
        chunk_data: text.as_bytes().to_vec(),
    };
    chuncks.insert(chuncks.len() - 1, text_chunck); // 만든 메타데이터를 끼워넣기

    Ok(chuncks)
}

fn making_png_data(chunks: Vec<Chunk>) -> bytes::Bytes {
    // png 파일 크기를 미리 계산하
    let size = chunks
        .iter()
        .fold(8, |total, cur| total + 4 + 4 + cur.chunk_data.len() + 4);
    let mut result = BytesMut::with_capacity(size); // 미리 용량을 할당
                                                    // println!("{size}");
    let signature: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];
    result.put_slice(&signature);

    for chunk in chunks {
        let length = chunk.chunk_data.len() as u32;
        let chunk_type = chunk.chunk_type.as_bytes();
        let chunk_data = chunk.chunk_data;

        let mut haser = crc32fast::Hasher::new();
        haser.update(&chunk_type);
        haser.update(&chunk_data);
        let crc = haser.finalize();

        result.put_u32(length);
        result.put_slice(chunk_type);
        result.put_slice(&chunk_data);
        result.put_u32(crc);
    }
    let png_file = result.freeze();

    png_file
}

fn jpg_to_png(path: &PathBuf) -> Result<String, Error> {
    let name = path;
    let parent = name.parent().unwrap().to_str().unwrap();
    let ext = name.extension().unwrap().to_string_lossy();
    let name = name
        .file_stem()
        .ok_or(anyhow!("Failed to read a filename"))?
        .to_string_lossy();
    let name = format!("./{parent}/temp/{name}.png");

    if ext != "avif" {
        let reader = ImageReader::open(path)?.with_guessed_format()?;
        let img = reader.decode()?;

        
        img.save_with_format(name.clone(), image::ImageFormat::Png)?;

        Ok(name)
    } else {
        avif_to_png(path, name.clone())?;
        Ok(name)
    }
}

// https://github.com/kornelski/avif-decode/blob/main/src/main.rs
fn avif_to_png(input_path: &PathBuf, output_path: String) -> Result<(), Error> {
    let data = std::fs::read(input_path)?;

    let d = Decoder::from_avif(&data)?;
    let encoded = match d.to_image()? {
        Image::Rgb8(img) => {
            let (buf, width, height) = img.into_contiguous_buf();
            lodepng::encode_memory(&buf, width, height, lodepng::ColorType::RGB, 8)
        },
        Image::Rgb16(img) => {
            let (mut buf, width, height) = img.into_contiguous_buf();
            buf.iter_mut().for_each(|px| {
                *px = px.map(|c| u16::from_ne_bytes(c.to_be_bytes()));
            });
            lodepng::encode_memory(&buf, width, height, lodepng::ColorType::RGB, 16)
        },
        Image::Rgba8(img) => {
            let (buf, width, height) = img.into_contiguous_buf();
            lodepng::encode_memory(&buf, width, height, lodepng::ColorType::RGBA, 8)
        },
        Image::Rgba16(img) => {
            let (mut buf, width, height) = img.into_contiguous_buf();
            buf.iter_mut().for_each(|px| {
                *px = px.map(|c| u16::from_ne_bytes(c.to_be_bytes()));
            });
            lodepng::encode_memory(&buf, width, height, lodepng::ColorType::RGBA, 16)
        },
        Image::Gray8(img) => {
            let (buf, width, height) = img.into_contiguous_buf();
            lodepng::encode_memory(&buf, width, height, lodepng::ColorType::GREY, 8)
        },
        Image::Gray16(img) => {
            let (mut buf, width, height) = img.into_contiguous_buf();
            buf.iter_mut().for_each(|px| {
                *px = px.map(|c| u16::from_ne_bytes(c.to_be_bytes()));
            });
            lodepng::encode_memory(&buf, width, height, lodepng::ColorType::GREY, 16)
        },
    }?;
    std::fs::write(output_path, encoded)?;
    Ok(())
}

pub fn convert_file(path: &PathBuf) -> Result<(), Error> {
    let img_path = jpg_to_png(path)?;
    let txt_path = path.with_extension("txt");

    let file_source = std::fs::File::open(txt_path)?;
    let result = parse_json_to_string(file_source)?;

    let mut image_file = std::fs::File::open(img_path)?;
    let mut contents = Vec::new();
    image_file.read_to_end(&mut contents)?;

    let chunks = add_text_chunck(contents, result)?;
    let new_file = making_png_data(chunks);

    let file_name = path.file_stem().unwrap().to_string_lossy();
    let new_dir = PathBuf::from(format!("./output/{file_name}.png"));
    if !new_dir.exists() {
        let mut file = std::fs::File::create(new_dir)?;
        file.write_all(&new_file)?;
    } else {
        println!("{new_dir:?} already exist.");
    }

    Ok(())
}

// #[test]
// fn test_something() -> Result<(), Error> {
//     use std::io::{Write, Read};

//     let file_source = std::fs::File::open("output.txt")?;
//     let result = parse_json_to_string(file_source);

//     let mut image_file = std::fs::File::open("output.png")?;
//     let mut contents = Vec::new();
//     image_file.read_to_end(&mut contents)?;

//     let chunks = add_text_chunck(contents, result)?;
//     let new_file = making_png_data(chunks);

//     let mut file = std::fs::File::create("999.png")?;
//     file.write_all(&new_file)?;

//     Ok(())
// }
