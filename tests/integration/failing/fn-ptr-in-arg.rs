fn main() {
    let bytes: [u8; 8] = [0x15, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
    let opt: Option<[u8; 8]> = Some(bytes);
    let result = opt.map(u64::from_le_bytes);
    assert_eq!(result, Some(21u64));
}
