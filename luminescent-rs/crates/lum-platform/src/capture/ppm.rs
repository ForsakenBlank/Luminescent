//! Reading the binary PPM (`P6`) that `grim -t ppm` writes.

use super::Frame;

/// The picture in a P6 file with 8-bit channels.
pub fn read(bytes: &[u8]) -> Result<Frame, String> {
    let bad = || "grim gave back a picture that could not be read".to_owned();
    let mut fields = Vec::new();
    let mut i = 0;
    // the header is four fields (P6, width, height, the largest value) between whitespace and comments
    while fields.len() < 4 {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if bytes.get(i) == Some(&b'#') {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        let start = i;
        while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if start == i {
            return Err(bad());
        }
        fields.push(std::str::from_utf8(&bytes[start..i]).map_err(|_| bad())?);
    }
    // exactly one whitespace byte separates the header from the pixels
    i += 1;
    let number = |s: &str| s.parse::<u32>().map_err(|_| bad());
    if fields[0] != "P6" || number(fields[3])? != 255 {
        return Err(bad());
    }
    let (w, h) = (number(fields[1])?, number(fields[2])?);
    let pixels = bytes.get(i..i + w as usize * h as usize * 3).ok_or_else(bad)?;
    let mut rgba = Vec::with_capacity(w as usize * h as usize * 4);
    for px in pixels.chunks_exact(3) {
        rgba.extend_from_slice(&[px[0], px[1], px[2], 255]);
    }
    Frame::new(w, h, rgba).ok_or_else(bad)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_small_picture_is_read() {
        let mut file = b"P6\n# made by a test\n2 1\n255\n".to_vec();
        file.extend_from_slice(&[255, 0, 0, 0, 0, 255]);
        let f = read(&file).expect("read");
        assert_eq!((f.width, f.height), (2, 1));
        assert_eq!(f.rgba, [255, 0, 0, 255, 0, 0, 255, 255]);
        assert!(read(b"P6\n2 1\n255\n\x01").is_err());
        assert!(read(b"P3\n1 1\n255\n1 2 3").is_err());
    }
}
