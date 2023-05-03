struct ControllerAPI {
    client: reqwest::Client,
    pub user: Option<String>,
}

impl ControllerAPI {
    pub fn new() -> Self {
        Self { client: reqwest::Client::new(), user: None }
    }
    /// Authenticate user by calling REST API
    /// Returns true if user is authenticated
    pub async fn login(&mut self, username: &str, password: &str) -> bool {
        let client = reqwest::Client::new();
        let url = format!("http://localhost:8080/authenticate?username={}&password={}", username, password);
        let response = client.get(&url).send().await;
        if response.is_err() {
            return false;
        }
        let response = response.unwrap();
        if response.status() != 200 {
            return false;
        }
        let response = response.text().await;
        if response.is_err() {
            return false;
        }
        let response = response.unwrap();
        if response != "true" {
            return false;
        }
        self.user = Some(username.to_string());
        true
    }
}