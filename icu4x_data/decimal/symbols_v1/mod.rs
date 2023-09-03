// @generated
type DataStruct =
    <::icu_decimal::provider::DecimalSymbolsV1Marker as ::icu_provider::DataMarker>::Yokeable;
pub fn lookup(locale: &icu_provider::DataLocale) -> Option<&'static DataStruct> {
    static KEYS: [&str; 2usize] = ["de-AT", "und"];
    static DATA: [&DataStruct; 2usize] = [&DE_AT, &UND];
    KEYS.binary_search_by(|k| locale.strict_cmp(k.as_bytes()).reverse())
        .ok()
        .map(|i| unsafe { *DATA.get_unchecked(i) })
}
static DE_AT: DataStruct = include!("de-AT.rs.data");
static UND: DataStruct = include!("und.rs.data");
