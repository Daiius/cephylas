
pub const MAX_LOG_LENGTH: usize = 8640;

pub struct LogCache {
    container: Vec<json::JsonValue>,
}

impl LogCache {
    pub fn new() -> Self {
        let mut v: Vec<json::JsonValue> = Vec::new();
        v.reserve(MAX_LOG_LENGTH);
        LogCache { container: v, }
    }
    pub fn add_and_rotate(self: &mut Self, d: json::JsonValue) {
        if self.container.len() >= MAX_LOG_LENGTH {
            self.container.rotate_left(1);
            self.container[MAX_LOG_LENGTH - 1] = d;
        } else {
            self.container.push(d);
        }
    }
    pub fn data(self: &Self) -> &Vec<json::JsonValue> { 
        &self.container
    }
}

