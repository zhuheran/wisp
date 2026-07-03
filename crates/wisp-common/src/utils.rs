use xxhash_rust::xxh3::Xxh3;
use uuid::Uuid;

pub fn compute_content_hash(content: &str) -> String {
    let mut hasher = Xxh3::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.digest()).to_uppercase()
}

pub fn get_uuid_v4() -> String {
    Uuid::new_v4().to_string().to_uppercase()
}
