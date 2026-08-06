use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use stego::{
    decode_png_hidden_pointer, encode_png_hidden_pointer_copy, ImageHiddenPointer,
    IMAGE_HIDDEN_CHECK_MARK_BYTES, IMAGE_HIDDEN_POINTER_BYTES,
};

#[test]
fn task_0660_image_copy_carries_pointer_and_preserves_original() {
    let temp = task_temp_dir();
    let original_path = temp.join("task0660-original.png");
    let copy_path = temp.join("task0660-copy.png");
    write_source_png(&original_path);

    let pointer = [
        0x06, 0x60, 0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x80, 0x90, 0xa0, 0xb0, 0xc0, 0xd0,
        0xe0, 0xf0, 0x0f, 0x1e, 0x2d,
    ];
    let check_mark = [0xca, 0xfe, 0x66, 0x00];
    assert_eq!(pointer.len(), IMAGE_HIDDEN_POINTER_BYTES);
    assert_eq!(check_mark.len(), IMAGE_HIDDEN_CHECK_MARK_BYTES);

    let original_hash_before = sha256_file(&original_path);
    let untouched_before = decode_png_hidden_pointer(&original_path).unwrap();

    encode_png_hidden_pointer_copy(&original_path, &copy_path, pointer, check_mark).unwrap();

    let decoded = decode_png_hidden_pointer(&copy_path)
        .unwrap()
        .expect("new image copy carries the OSL image-hidden frame");
    let original_hash_after = sha256_file(&original_path);
    let untouched_after = decode_png_hidden_pointer(&original_path).unwrap();

    println!("TASK0660 pointer_hex={}", hex(&pointer));
    println!("TASK0660 check_mark_hex={}", hex(&check_mark));
    println!("TASK0660 decoded_pointer_hex={}", hex(&decoded.pointer));
    println!(
        "TASK0660 decoded_check_mark_hex={}",
        hex(&decoded.check_mark)
    );
    println!(
        "TASK0660 pointer_byte_for_byte={}",
        decoded.pointer == pointer
    );
    println!(
        "TASK0660 check_mark_byte_for_byte={}",
        decoded.check_mark == check_mark
    );
    println!("TASK0660 original_hash_before={original_hash_before}");
    println!("TASK0660 original_hash_after={original_hash_after}");
    println!(
        "TASK0660 original_hash_unchanged={}",
        original_hash_before == original_hash_after
    );
    println!(
        "TASK0660 untouched_original_decode={}",
        if untouched_after.is_none() {
            "None"
        } else {
            "Some"
        }
    );
    println!("TASK0660 new_copy_path={}", copy_path.display());

    assert_eq!(untouched_before, None);
    assert_eq!(decoded, ImageHiddenPointer::new(pointer, check_mark));
    assert_eq!(original_hash_before, original_hash_after);
    assert_eq!(untouched_after, None);
}

fn task_temp_dir() -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "osl-task-0660-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    if path.exists() {
        fs::remove_dir_all(&path).unwrap();
    }
    fs::create_dir_all(&path).unwrap();
    path
}

fn write_source_png(path: &Path) {
    let width = 12;
    let height = 8;
    let mut pixels = Vec::with_capacity(width * height * 3);
    for y in 0..height {
        for x in 0..width {
            pixels.push((17 + x * 11 + y * 3) as u8);
            pixels.push((91 + x * 5 + y * 13) as u8);
            pixels.push((203u16.wrapping_sub((x * 7 + y * 9) as u16) & 0xff) as u8);
        }
    }

    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(Cursor::new(&mut bytes), width as u32, height as u32);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&pixels)
            .unwrap();
    }
    fs::write(path, bytes).unwrap();
}

fn sha256_file(path: &Path) -> String {
    let bytes = fs::read(path).unwrap();
    hex(&Sha256::digest(bytes))
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}
