use super::*;
#[derive(Debug, PartialEq, zipzi_api_derive::BinaryWire)]
struct Packet {
    name: String,
    points: Vec<[f32; 2]>,
    labels: BTreeMap<usize, Option<String>>,
    extension: serde_json::Value,
    #[serde(skip)]
    imported: Vec<f32>,
}
#[derive(Debug, PartialEq, zipzi_api_derive::BinaryWire)]
enum Tagged {
    Unit,
    Packet(Packet),
    Fields { bytes: Vec<u8>, enabled: bool },
}
#[test]
fn owning_indirection_preserves_layout_and_admits_its_allocation() {
    let value = vec![1u32, 7, 99];
    let bytes = encode(&value).unwrap();
    assert_eq!(Box::<Vec<u32>>::SIGNATURE, Vec::<u32>::SIGNATURE);
    assert_eq!(encode(&Box::new(value.clone())).unwrap(), bytes);
    let mut plain = Reader::new(&bytes).unwrap();
    let cost = Vec::<u32>::input_cost(&mut plain).unwrap();
    let mut measured = Reader::new(&bytes).unwrap();
    assert!(Box::<Vec<u32>>::input_cost(&mut measured).unwrap() > cost);
    measured.finish().unwrap();
    let mut input = Reader::new(&bytes).unwrap();
    assert_eq!(*Box::<Vec<u32>>::decode(&mut input).unwrap(), value);
    input.finish().unwrap();
}
#[test]
fn concrete_records_roundtrip_without_json_and_skip_import_only_fields() {
    let value = Tagged::Packet(Packet {
        name: "表情 🦊".into(),
        points: vec![[1.25, -2.5], [3., 4.]],
        labels: BTreeMap::from([(7, Some("seven".into())), (19, None)]),
        extension: serde_json::json!({"a":[true,null,-3,1.5],"text":"x"}),
        imported: vec![99.],
    });
    let bytes = encode(&value).unwrap();
    assert_eq!(bytes.len(), encoded_len(&value).unwrap());
    let mut measured = Reader::new(&bytes).unwrap();
    let cost = Tagged::input_cost(&mut measured).unwrap();
    measured.finish().unwrap();
    assert!(cost >= bytes.len());
    let mut input = Reader::new(&bytes).unwrap();
    let decoded = Tagged::decode(&mut input).unwrap();
    input.finish().unwrap();
    let Tagged::Packet(mut expected) = value else {
        panic!()
    };
    expected.imported.clear();
    assert_eq!(decoded, Tagged::Packet(expected));
    assert_eq!(Tagged::VARIANTS, &["Unit", "Packet", "Fields"]);
}
#[test]
fn byte_vectors_use_bulk_lengths_beyond_the_structural_item_limit() {
    let value = Tagged::Fields {
        bytes: vec![83; MAX_ELEMENTS + 17],
        enabled: true,
    };
    let bytes = encode(&value).unwrap();
    let mut input = Reader::new(&bytes).unwrap();
    Tagged::input_cost(&mut input).unwrap();
    input.finish().unwrap();
    let mut input = Reader::new(&bytes).unwrap();
    let Tagged::Fields {
        bytes: decoded,
        enabled,
    } = Tagged::decode(&mut input).unwrap()
    else {
        panic!()
    };
    assert!(enabled);
    assert_eq!(decoded.len(), MAX_ELEMENTS + 17);
    assert_eq!(decoded.capacity(), decoded.len());
}
#[test]
fn malformed_lengths_floats_utf8_and_tags_are_rejected_in_the_cost_pass() {
    for payload in [
        vec![255, 255, 255, 255],
        vec![2, 0, 0, 0, 255],
        vec![1, 0, 0, 0, 0xff],
    ] {
        let mut input = Reader::new(&payload).unwrap();
        assert!(String::input_cost(&mut input).is_err());
    }
    let mut input = Reader::new(&[255, 255, 255, 255]).unwrap();
    assert!(Vec::<f32>::input_cost(&mut input).is_err());
    let nan = f32::NAN.to_le_bytes();
    assert!(f32::input_cost(&mut Reader::new(&nan).unwrap()).is_err());
    assert!(bool::input_cost(&mut Reader::new(&[2]).unwrap()).is_err());
    assert!(Option::<u32>::input_cost(&mut Reader::new(&[3]).unwrap()).is_err());
    assert!(Tagged::input_cost(&mut Reader::new(&99u32.to_le_bytes()).unwrap()).is_err());
    assert!(Reader::new(&[1]).unwrap().finish().is_err());
}
