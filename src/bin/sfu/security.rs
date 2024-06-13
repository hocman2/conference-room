// A bunch of stuff used to determine if the SFU should run in secure mode or not

use std::env;

const SSL_MODE_ENV_KEY: &str = "SSL_MODE";
const SSL_CERT_PATH_ENV_KEY: &str = "SSL_CERT_PATH";
const SSL_KEY_PATH_ENV_KEY: &str = "SSL_KEY_PATH";
pub struct SSLModeSettings {
	pub cert_path: String,
	pub key_path: String
}

pub fn get_ssl_mode_settings() -> Option<SSLModeSettings> {
	let secure_mode = match env::var(SSL_MODE_ENV_KEY) {
    	Ok(val) =>  match val.parse::<i32>() {
     		Ok(val) => val > 0,
       		Err(e) => panic!("Error parsing SSL_MODE value: {e}")
     	},
    	Err(_) => {
     		println!("{SSL_MODE_ENV_KEY} was not found. Running in non-secure mode.");
     		println!("The environment variable {SSL_MODE_ENV_KEY}=1 is required on an environment using HTTPS");
       		false
     	}
    };

	if !secure_mode {
		return None;
	}

	let cert_path = match env::var(SSL_CERT_PATH_ENV_KEY) {
		Ok(path) => path,
		Err(_) => panic!("The {SSL_CERT_PATH_ENV_KEY} environment variable is required when {SSL_MODE_ENV_KEY}=1")
	};

	let key_path = match env::var(SSL_KEY_PATH_ENV_KEY) {
		Ok(path) => path,
		Err(_) => panic!("The {SSL_KEY_PATH_ENV_KEY} environment variable is required when {SSL_MODE_ENV_KEY}=1")
	};

	Some(SSLModeSettings {
		cert_path,
		key_path
	})
}
