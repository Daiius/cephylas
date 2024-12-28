
use cephylas::time;

#[test]
fn convert_and_back_now() {

    let now = std::time::SystemTime::now();
    
    let converted = cephylas::time::format_time(&now);
    let converted_back = cephylas::time::parse_time(&converted);
    assert!(now == converted_back);
}
