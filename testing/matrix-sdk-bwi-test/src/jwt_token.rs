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

pub struct ExamplePrivateRSAKey {
    pub key: &'static str,
}

pub struct ExamplePublicRSAKey {
    pub key: &'static str,
}

const RSA_PRIVATE_EXAMPLE_KEY_AS_STR: &str = r"-----BEGIN RSA PRIVATE KEY-----
MIIEpAIBAAKCAQEA4S9KCCX4HoVSuzI+Dj3RwNI8FqbDSXgqHMR7/sNEuONqiZI0
g5qvbJdXS/Ph8iLhp5nHTINPha9vuFgo4svcpksftDYCBOeXK/PnUIj4BjZtTZQd
4J2SlYjITGde52Bkr+gZmfC61XoQzpouf+PAXqD8uWGFkFYx68U3ImC9b0/HARW0
9/T2y22EhKdzPg5XnsNkbKORot7ri9xzgRho1s1Mo3ZUQMf888ENAD1bodMfp4H1
oQH14EJviLe5lyadtS+G8pXDgJjJrXRfL4xARPHmfG3DBVEt0AVrJ/YK/15vZGEy
XosLKQhaJ1tKB5YiKe8AVVaK8NWL4Ssi5NZPMwIDAQABAoIBAQC///kof29SXq0O
raUZsX4R4W+YhLIIB6wogSOUMlx7JHSnPgEGoTKn7VZijVx+f11V8RlsVJ3OR6qD
TK/3yIinRXCP0GjiU7CiqkD1ewO3EFscBYmABfqBA3J9KrpRn+/ZyJVjm01CTeTc
s7uoEcHpLWyuh8dBLS09cRc0ebWrnImKXFL/6sXyREG/Xk87ID5UTfy+M/+ZmwBk
+WBf+ipXgIvtr3DcC8WdveGsn/Gn2bjEgc37jhfqFdyG8a3bG8fEKV6W7+LjOIVV
kKUr9+vFHb71KGPgCsX6T6ex+pk3Y+JVrV12M5rNovsbQcgjFmH3WBQ1a5yyjvxV
wlwT0I2BAoGBAPBrRPpWAZFoBbvGmMR9n587kFx/dqCvioGmpTXo0Al6woMQWNX1
Y5dIMW+1q6Xt6TBRYfsW/xrX9rqehBTGZ0WBesIDOIwP+oG7t5/NOBarmGXyDeA/
gloQEHakcyomKcqs1auu8xZvq8AP+pTS3EXbsQeTx0nPRUD3qMzbmqtzAoGBAO/H
RXpZ1SHmjK/KJa/zM1p0OzQuZFW/AO7Ftgb5gn6M/VNTMLR2Pjac7QtKjXdXuy8o
aOlBtn8X7/n/YVkZzY5vnZM9bYIxZONhEXd6M54xki6UvZG6HjQAzXpRUR6fhY0J
wPUe7JKzbOFr511v/zGywGnOpjX+PQzmhNfCmF1BAoGATXaClxY3Ex6tGj924XiI
gcmzTdpT4posynFjMed9gFBpc8lElkumdwvvwcqLL79kLwlJxJk4QPHssVx5uifj
BmYdo31eLuLHGB3foEGDHOrVA6PmDKbp3RLn+xIpeR8qv/7IKbUI5eW9NPjxCBqY
lnmepI5c289IxRIG9VqcjzkCgYBF1jRWPnPlO9EeIjJ33M3IOrJDsH9ougj7gnpR
7bokQcxGyKQW65mTLoGcGEq7x8GtKofj6E/PFJnAprEj0nAcXEX47JtIoDpSP6Nm
uSDvomCBULEmEJ9bZiByz9xgnvW27nBU9HzS/Y9o2JS6kjQxtW51YsrmTvZZG4r1
jKf0AQKBgQC6bRghEzPvNqX7ZzaLrfwgmbx0yHpSLVwLjiYle9EFDf7K1wVZRTmx
6MVAkpT5rGQFhgPcXN39U+z5g3k/NWc7DKhKlWuTuMmr5gwpzQBt37d3pTKc4QFF
TA/8TRVkK6rRp7qXJ+gq8v3mSUFhwwGY9zETQ/k/nwiKH6RYXFsMiw==
-----END RSA PRIVATE KEY-----
";

const RSA_PUBLIC_EXAMPLE_KEY_AS_STR: &str = r"-----BEGIN PUBLIC KEY-----
MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEA4S9KCCX4HoVSuzI+Dj3R
wNI8FqbDSXgqHMR7/sNEuONqiZI0g5qvbJdXS/Ph8iLhp5nHTINPha9vuFgo4svc
pksftDYCBOeXK/PnUIj4BjZtTZQd4J2SlYjITGde52Bkr+gZmfC61XoQzpouf+PA
XqD8uWGFkFYx68U3ImC9b0/HARW09/T2y22EhKdzPg5XnsNkbKORot7ri9xzgRho
1s1Mo3ZUQMf888ENAD1bodMfp4H1oQH14EJviLe5lyadtS+G8pXDgJjJrXRfL4xA
RPHmfG3DBVEt0AVrJ/YK/15vZGEyXosLKQhaJ1tKB5YiKe8AVVaK8NWL4Ssi5NZP
MwIDAQAB
-----END PUBLIC KEY-----
";

static RSA_PRIVATE_EXAMPLE_KEY: ExamplePrivateRSAKey =
    ExamplePrivateRSAKey { key: RSA_PRIVATE_EXAMPLE_KEY_AS_STR };

static RSA_PUBLIC_EXAMPLE_KEY: ExamplePublicRSAKey =
    ExamplePublicRSAKey { key: RSA_PUBLIC_EXAMPLE_KEY_AS_STR };

/// Get
pub fn get_example_rsa_keys_in_pem_format(
) -> (&'static ExamplePrivateRSAKey, &'static ExamplePublicRSAKey) {
    (&RSA_PRIVATE_EXAMPLE_KEY, &RSA_PUBLIC_EXAMPLE_KEY)
}
