use ipc::transport_padding::pad_transport_object;

#[test]
fn t1_34_shipping_ipc_exports_the_transport_padding_boundary() {
    let padded = pad_transport_object(vec![0x5a; 1_001]).expect("representable Padmé length");
    assert_eq!(padded.len(), 1_024);
}
