use std::io::Cursor;

use stego::{
    decode_png_hidden_pointer_bytes, encode_png_hidden_pointer_bytes, IMAGE_HIDDEN_POINTER_BYTES,
};

#[derive(Clone, Copy)]
struct ProviderProfile {
    name: &'static str,
    max_long_edge: u32,
}

struct DecodedImage {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

#[test]
fn task_0666_provider_shrink_and_resave_preserves_hidden_pointer() {
    let sent_pointer = [
        0x06, 0x66, 0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x80, 0x90, 0xa0, 0xb0, 0xc0, 0xd0,
        0xe0, 0xf0, 0x0f, 0x1e, 0x2d,
    ];
    assert_eq!(sent_pointer.len(), IMAGE_HIDDEN_POINTER_BYTES);
    let check_mark = [0xca, 0xfe, 0x66, 0x60];
    let source = source_png(2048, 768);
    let prepared = encode_png_hidden_pointer_bytes(&source, sent_pointer, check_mark)
        .expect("prepared image carries the hidden pointer");

    println!("TASK0666_SENT_POINTER_HEX={}", hex(&sent_pointer));

    let profiles = [
        ProviderProfile {
            name: "TELEGRAM",
            max_long_edge: 1280,
        },
        ProviderProfile {
            name: "INSTAGRAM",
            max_long_edge: 240,
        },
        ProviderProfile {
            name: "SIGNAL",
            max_long_edge: 1600,
        },
    ];

    for profile in profiles {
        let resaved = shrink_and_resave_png(&prepared, profile.max_long_edge);
        let resaved_image = decode_png_rgb(&resaved);
        println!(
            "TASK0666_{}_RESAVED_DIMENSIONS={}x{}",
            profile.name, resaved_image.width, resaved_image.height
        );
        let decoded = decode_png_hidden_pointer_bytes(&resaved)
            .expect("provider-resaved image decodes")
            .unwrap_or_else(|| {
                panic!(
                    "TASK0667_{}_DECODE_CHECK=red provider-resaved image no longer contains an OSL pointer",
                    profile.name
                )
            });
        let matches = decoded.pointer == sent_pointer;

        println!(
            "TASK0666_{}_DECODED_POINTER_HEX={}",
            profile.name,
            hex(&decoded.pointer)
        );
        println!("TASK0666_{}_MATCH={matches}", profile.name);

        assert_eq!(decoded.pointer, sent_pointer);
        assert_eq!(decoded.check_mark, check_mark);
    }
}

fn source_png(width: u32, height: u32) -> Vec<u8> {
    let mut pixels = Vec::with_capacity((width * height * 3) as usize);
    for y in 0..height {
        for x in 0..width {
            pixels.push(((31 + x * 3 + y * 5) & 0xff) as u8);
            pixels.push(((89 + x * 7 + y * 11) & 0xff) as u8);
            pixels.push(((173 + x * 13 + y * 17) & 0xff) as u8);
        }
    }
    write_png_rgb(width, height, &pixels)
}

fn shrink_and_resave_png(source: &[u8], max_long_edge: u32) -> Vec<u8> {
    let source = decode_png_rgb(source);
    let longest = source.width.max(source.height);
    assert!(longest > max_long_edge, "fixture must exercise shrink");
    let target_width =
        ((u64::from(source.width) * u64::from(max_long_edge)) / u64::from(longest)) as u32;
    let target_height =
        ((u64::from(source.height) * u64::from(max_long_edge)) / u64::from(longest)) as u32;

    let mut out = vec![0u8; (target_width * target_height * 3) as usize];
    for y in 0..target_height {
        let sy0 = (u64::from(y) * u64::from(source.height) / u64::from(target_height)) as u32;
        let sy1 = ((u64::from(y + 1) * u64::from(source.height) / u64::from(target_height)) as u32)
            .max(sy0 + 1)
            .min(source.height);
        for x in 0..target_width {
            let sx0 = (u64::from(x) * u64::from(source.width) / u64::from(target_width)) as u32;
            let sx1 = ((u64::from(x + 1) * u64::from(source.width) / u64::from(target_width))
                as u32)
                .max(sx0 + 1)
                .min(source.width);
            let mut total = [0u64; 3];
            let mut count = 0u64;
            for sy in sy0..sy1 {
                for sx in sx0..sx1 {
                    let source_offset = ((sy * source.width + sx) * 3) as usize;
                    total[0] += u64::from(source.pixels[source_offset]);
                    total[1] += u64::from(source.pixels[source_offset + 1]);
                    total[2] += u64::from(source.pixels[source_offset + 2]);
                    count += 1;
                }
            }
            let target_offset = ((y * target_width + x) * 3) as usize;
            out[target_offset] = (total[0] / count) as u8;
            out[target_offset + 1] = (total[1] / count) as u8;
            out[target_offset + 2] = (total[2] / count) as u8;
        }
    }

    write_png_rgb(target_width, target_height, &out)
}

fn decode_png_rgb(bytes: &[u8]) -> DecodedImage {
    let decoder = png::Decoder::new(Cursor::new(bytes));
    let mut reader = decoder.read_info().expect("PNG header reads");
    let mut pixels = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut pixels).expect("PNG frame reads");
    pixels.truncate(info.buffer_size());
    assert_eq!(info.bit_depth, png::BitDepth::Eight);

    let rgb = match info.color_type {
        png::ColorType::Rgb => pixels,
        png::ColorType::Rgba => pixels
            .chunks_exact(4)
            .flat_map(|chunk| [chunk[0], chunk[1], chunk[2]])
            .collect(),
        other => panic!("unsupported fixture color type: {other:?}"),
    };

    DecodedImage {
        width: info.width,
        height: info.height,
        pixels: rgb,
    }
}

fn write_png_rgb(width: u32, height: u32, pixels: &[u8]) -> Vec<u8> {
    assert_eq!(pixels.len(), (width * height * 3) as usize);
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(Cursor::new(&mut bytes), width, height);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .expect("PNG header writes")
            .write_image_data(pixels)
            .expect("PNG pixels write");
    }
    bytes
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}
