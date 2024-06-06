pub mod encoding;
#[allow(clippy::unnecessary_cast)]
pub mod json_stream;
pub mod partial_json;

pub trait ZType<T> {
    fn z_type(self) -> T;
}
impl ZType<u32> for u64 {
    fn z_type(self) -> u32 {
        self as u32
    }
}
impl ZType<u64> for u64 {
    fn z_type(self) -> u64 {
        self
    }
}
