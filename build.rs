fn main() {
    // Init the export table with entries from the real dxgi.dll, ensuring that they automatically
    // forward upon execution.
    forward_dll::forward_dll("C:\\Windows\\System32\\dxgi.dll").unwrap();
}