use axum::http::HeaderValue;

pub fn make_refresh_cookie(token: &str, max_age_secs: u64, secure: bool) -> HeaderValue {
    let secure_flag = if secure { "; Secure" } else { "" };
    HeaderValue::from_str(&format!(
        "refresh_token={token}; HttpOnly; SameSite=Lax; Path=/api/auth; Max-Age={max_age_secs}{secure_flag}"
    ))
    .expect("valid cookie header")
}

pub fn make_clear_refresh_cookie(secure: bool) -> HeaderValue {
    let secure_flag = if secure { "; Secure" } else { "" };
    HeaderValue::from_str(&format!(
        "refresh_token=; HttpOnly; SameSite=Lax; Path=/api/auth; Max-Age=0{secure_flag}"
    ))
    .expect("valid cookie header")
}

pub fn extract_refresh_token_from_cookie_header(header: &str) -> Option<&str> {
    header.split(';').find_map(|pair| {
        let pair = pair.trim();
        pair.strip_prefix("refresh_token=")
    })
}
