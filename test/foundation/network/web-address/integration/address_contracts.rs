use url_encoding::{address::Url, form::Serializer};

#[test]
fn blob_origins_follow_the_embedded_scheme() {
    let https = Url::parse("blob:https://example.org:8443/photo").unwrap();
    assert_eq!(
        https.origin().ascii_serialization(),
        "https://example.org:8443"
    );

    let ftp = Url::parse("blob:ftp://example.org/photo").unwrap();
    assert!(!ftp.origin().is_tuple());
}

#[test]
fn article_links_keep_the_path_and_query_encodings_distinct() {
    let mut article = Url::parse("https://example.org/notes/")
        .unwrap()
        .join("a^2")
        .unwrap();
    article.query_pairs_mut().append_pair("tag", "C++ basics");
    assert_eq!(
        article.as_str(),
        "https://example.org/notes/a%5E2?tag=C%2B%2B+basics"
    );
}

#[test]
fn form_suffix_starts_at_the_end_of_a_shortened_prefix() {
    let mut prefix = String::from("/search/?");
    let start = prefix.len();
    prefix.replace_range(7..8, "");
    let query = Serializer::for_suffix(prefix, start)
        .append_pair("q", "iron ore")
        .finish();
    assert_eq!(query, "/search?q=iron+ore");
}
