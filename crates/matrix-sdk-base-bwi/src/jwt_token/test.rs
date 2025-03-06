/*
 * Copyright (c) 2024 BWI GmbH
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

mod utils {
    use crate::jwt_token::BWIJwtToken;
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
    use matrix_sdk_bwi_test::jwt_token::{
        get_example_rsa_keys_in_pem_format, ExamplePrivateRSAKey, ExamplePublicRSAKey,
    };
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    pub fn get_rsa_keys() -> (&'static ExamplePrivateRSAKey, &'static ExamplePublicRSAKey) {
        get_example_rsa_keys_in_pem_format()
    }

    pub trait Encode {
        fn encode_with_private_key(self, private_key_in_pem_format: &str) -> String;
    }

    impl Encode for BWIJwtToken {
        fn encode_with_private_key(self, private_key_in_pem_format: &str) -> String {
            encode(
                &Header::new(Algorithm::PS512),
                &self,
                &EncodingKey::from_rsa_pem(String::from(private_key_in_pem_format).as_bytes())
                    .unwrap(),
            )
            .unwrap()
        }
    }

    pub fn one_hour_from_now() -> f64 {
        let one_hour = Duration::from_secs(3600);
        let one_hour_from_now = SystemTime::now() + one_hour;
        one_hour_from_now.duration_since(UNIX_EPOCH).unwrap().as_secs() as f64
    }

    pub fn one_hour_before_now() -> f64 {
        let one_hour = Duration::from_secs(3600);
        let one_hour_from_now = SystemTime::now() - one_hour;
        one_hour_from_now.duration_since(UNIX_EPOCH).unwrap().as_secs() as f64
    }
}

mod test_expiration_date_claim {
    use crate::jwt_token::test::utils::{
        get_rsa_keys, one_hour_before_now, one_hour_from_now, Encode,
    };
    use crate::jwt_token::{BWIJwtToken, BWIPublicKeyForJWTTokenValidation, BWITokenValidator};
    use std::ops::Not;

    const HOMESERVER_DOMAIN: &str = "examplemessenger.de";
    const HOMESERVER_DOMAIN_URL: &str = "https://examplemessenger.de";

    #[test]
    fn test_valid_json_token() {
        // Arrange
        let (private_key, public_key) = get_rsa_keys();

        let encoded_token =
            BWIJwtToken { exp: one_hour_from_now(), sub: String::from(HOMESERVER_DOMAIN) }
                .encode_with_private_key(private_key.key);

        // Act
        let validator = BWITokenValidator::for_homeserver(HOMESERVER_DOMAIN_URL.parse().unwrap());

        let decryption_key =
            BWIPublicKeyForJWTTokenValidation::from_string(public_key.key).unwrap();

        let has_valid_key = validator.validate_jwt_token_with_key(&encoded_token, &decryption_key);

        // Assert
        assert!(has_valid_key)
    }

    #[test]
    fn test_expired_json_token() {
        // Arrange
        let (private_key, public_key) = get_rsa_keys();

        let encoded_token =
            BWIJwtToken { exp: one_hour_before_now(), sub: String::from(HOMESERVER_DOMAIN) }
                .encode_with_private_key(private_key.key);

        // Act
        let validator = BWITokenValidator::for_homeserver(HOMESERVER_DOMAIN_URL.parse().unwrap());

        let decryption_key =
            BWIPublicKeyForJWTTokenValidation::from_string(&public_key.key).unwrap();

        let has_valid_key = validator.validate_jwt_token_with_key(&encoded_token, &decryption_key);

        // Assert
        assert!(has_valid_key.not())
    }

    #[test]
    fn test_one_expired_json_token_and_one_valid_token() {
        // Arrange
        let (private_key, public_key) = get_rsa_keys();

        let encoded_expired_token =
            BWIJwtToken { exp: one_hour_before_now(), sub: String::from(HOMESERVER_DOMAIN) }
                .encode_with_private_key(private_key.key);

        let encoded_valid_token =
            BWIJwtToken { exp: one_hour_from_now(), sub: String::from(HOMESERVER_DOMAIN) }
                .encode_with_private_key(private_key.key);

        // Act
        let validator = BWITokenValidator::for_homeserver(HOMESERVER_DOMAIN_URL.parse().unwrap());

        let decryption_key =
            BWIPublicKeyForJWTTokenValidation::from_string(&public_key.key).unwrap();

        let has_valid_key = validator.validate_jwt_tokens_with_keys(
            &mut vec![encoded_expired_token, encoded_valid_token].into_iter(),
            &vec![decryption_key],
        );

        // Assert
        assert!(has_valid_key)
    }
}

mod test_homeserver_url_claim {
    use crate::jwt_token::test::utils::{
        get_rsa_keys, one_hour_before_now, one_hour_from_now, Encode,
    };
    use crate::jwt_token::{BWIJwtToken, BWIPublicKeyForJWTTokenValidation, BWITokenValidator};
    use std::ops::Not;

    const CORRECT_HOMESERVER_DOMAIN: &str = "correcthomeserver.de";
    const CORRECT_HOMESERVER_DOMAIN_URL: &str = "https://correcthomeserver.de";

    const INCORRECT_HOMESERVER_DOMAIN: &str = "incorrecthomeserver.de";

    #[test]
    fn test_correct_homserver() {
        // Arrange
        let (private_key, public_key) = get_rsa_keys();

        let encoded_token =
            BWIJwtToken { exp: one_hour_from_now(), sub: String::from(CORRECT_HOMESERVER_DOMAIN) }
                .encode_with_private_key(private_key.key);

        // Act
        let validator =
            BWITokenValidator::for_homeserver(CORRECT_HOMESERVER_DOMAIN_URL.parse().unwrap());

        let decryption_key =
            BWIPublicKeyForJWTTokenValidation::from_string(public_key.key).unwrap();

        let has_valid_key = validator.validate_jwt_token_with_key(&encoded_token, &decryption_key);

        // Assert
        assert!(has_valid_key)
    }

    #[test]
    fn test_incorrect_homeserver() {
        // Arrange
        let (private_key, public_key) = get_rsa_keys();

        let encoded_token = BWIJwtToken {
            exp: one_hour_before_now(),
            sub: String::from(INCORRECT_HOMESERVER_DOMAIN),
        }
        .encode_with_private_key(private_key.key);

        // Act
        let validator =
            BWITokenValidator::for_homeserver(CORRECT_HOMESERVER_DOMAIN_URL.parse().unwrap());

        let decryption_key =
            BWIPublicKeyForJWTTokenValidation::from_string(public_key.key).unwrap();

        let has_valid_key = validator.validate_jwt_token_with_key(&encoded_token, &decryption_key);

        // Assert
        assert!(has_valid_key.not())
    }
}
