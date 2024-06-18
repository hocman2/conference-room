// A bunch of stuff used to determine if the SFU should run in secure mode or not

use std::env;

use log::error;

const TLS_MODE_ENV_KEY: &str = "TLS_MODE";
const TLS_CERT_PATH_ENV_KEY: &str = "TLS_CERT_PATH";
const TLS_KEY_PATH_ENV_KEY: &str = "TLS_KEY_PATH";
pub struct TLSModeSettings {
	pub cert_path: String,
	pub key_path: String
}

pub fn get_tls_mode_settings() -> Option<TLSModeSettings> {
	let secure_mode = match env::var(TLS_MODE_ENV_KEY) {
    	Ok(val) =>  match val.parse::<i32>() {
     		Ok(val) => val > 0,
       		Err(e) => {
         		println!("{val} is not a valid SSL_MODE value");
           		error!("Error parsing SSL_MODE value: {e}");
             	return None;
         	}
     	},
    	Err(_) => {
     		println!("{TLS_MODE_ENV_KEY} was not found. Running in non-secure mode.");
     		println!("The environment variable {TLS_MODE_ENV_KEY}=1 is required on an environment using HTTPS");
       		false
     	}
    };

	if !secure_mode {
		return None;
	}

	let cert_path = match env::var(TLS_CERT_PATH_ENV_KEY) {
		Ok(path) => path,
		Err(_) => {
			println!("The {TLS_CERT_PATH_ENV_KEY} environment variable is required when {TLS_MODE_ENV_KEY}=1");
			return None;
		}
	};

	let key_path = match env::var(TLS_KEY_PATH_ENV_KEY) {
		Ok(path) => path,
		Err(_) => {
			println!("The {TLS_KEY_PATH_ENV_KEY} environment variable is required when {TLS_MODE_ENV_KEY}=1");
			return None;
		}
	};

	Some(TLSModeSettings {
		cert_path,
		key_path
	})
}
