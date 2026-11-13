use super::*;

#[test]
fn peak_is_the_loudest_sample_in_hundredths() {
    let samples = |values: &[i16]| {
        values
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect::<Vec<_>>()
    };
    assert_eq!(peak(&samples(&[0, 0])), 0.0);
    assert_eq!(peak(&samples(&[100, -16384, 5])), 0.5);
    assert_eq!(peak(&samples(&[i16::MIN])), 1.0);
    assert_eq!(peak(&samples(&[i16::MAX])), 1.0);
    assert_eq!(peak(&[]), 0.0);
}
