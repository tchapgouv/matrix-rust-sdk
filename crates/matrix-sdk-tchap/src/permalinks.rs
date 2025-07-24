use ruma::MatrixToUri;

/// Sadly, const `MATRIX_TO_BASE_URL` is private in ruma::matrix_uri.
/// So, we need do declare a new one here.
const TCHAP_TO_BASE_URL: &str = "https://tchap.gouv.fr/#/";
const MATRIX_TO_BASE_URL: &str = "https://matrix.to/#/";
#[allow(dead_code)]
const TCHAP_SCHEME: &str = "tchap";

/// For use on MatrixId enum types parameter (that are `str` type`):
/// - room ID: Room(OwnedRoomId)
/// - room alias: RoomAlias(OwnedRoomAliasId)
/// - user ID: User(OwnedUserId)
/// - event ID: Event(OwnedRoomOrAliasId, OwnedEventId)
/// - or any Matrix entity
trait StringConvertTchapExt {
    /// Returns a str where `https://matrix.to/#/` was replaced by `https://tchap.gouv.fr/#/`.
    fn replace_matrix_by_tchap(&self) -> String;

    /// Returns a str where `https://tchap.gouv.fr/#/` was replaced by `https://matrix.to/#/`.
    fn replace_tchap_by_matrix(&self) -> String;
}

/// Implementation of StringConvertTchapExt for `str` because
/// MatrixID enum cases are based on `str`.
impl StringConvertTchapExt for str {
    fn replace_matrix_by_tchap(&self) -> String {
        self.to_string().replace(MATRIX_TO_BASE_URL, TCHAP_TO_BASE_URL)
    }

    fn replace_tchap_by_matrix(&self) -> String {
        self.to_string().replace(TCHAP_TO_BASE_URL, MATRIX_TO_BASE_URL)
    }
}

pub trait MatrixToUriToTchapString {
    fn to_tchap_string(self) -> String;
}

impl MatrixToUriToTchapString for MatrixToUri {
    /// Convert to string a MatrixToUri (whose `id` is a str beginning with `https://matrix.to/#/`)
    /// to a Tchap MatrixID string (i.e. a String beginning with `https://tchap.gouv.fr/#/`).
    fn to_tchap_string(self) -> String {
        self.to_string().replace_matrix_by_tchap()
    }
}

pub trait TchapStringForMatrixUri {
    fn to_matrix_string(self) -> String;
}

impl TchapStringForMatrixUri for &str {
     /// Convert a Tchap MatrixID (i.e. a `str` beginning with `https://tchap.gouv.fr/#/`)
    /// to a MatrixID (i.e. a `str`` beginning with `https://matrix.to/#/`).
    fn to_matrix_string(self) -> String {
        self.replace_tchap_by_matrix()
    }
}