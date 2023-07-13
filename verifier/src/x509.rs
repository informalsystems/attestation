// Copyright (c) 2023 The MobileCoin Foundation

extern crate alloc;
use alloc::string::{String, ToString};
use alloc::vec;
use core::fmt::{Debug, Formatter};

use mbedtls::{
    alloc::List as MbedtlsList,
    hash::Type as HashType,
    pk::{EcGroupId, Type as PkType},
    x509::{Certificate, Crl, KeyUsage, Profile},
};

use x509_cert::{der::Decode, Certificate as X509Certificate};

pub type Result<T> = core::result::Result<T, Error>;

/// Error type for decoding and verifying certificates.
#[derive(Debug, displaydoc::Display, PartialEq, Eq)]
pub enum Error {
    /// An error occurred working with MbedTls: {0}
    MbedTls(mbedtls::Error),
}

impl From<mbedtls::Error> for Error {
    fn from(src: mbedtls::Error) -> Self {
        Error::MbedTls(src)
    }
}

/// Trust anchor for a certificate chain.
#[derive(Clone)]
pub struct TrustAnchor(MbedtlsList<Certificate>);

impl Debug for TrustAnchor {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f, "TrustAnchor{{...}}")
    }
}

impl TrustAnchor {
    /// Try to get a trust anchor from a PEM encoded string.
    ///
    /// # Errors
    /// `Error::MbedTls` if the string is not valid PEM certificate.
    pub fn try_from_pem(pem: impl Into<String>) -> Result<Self> {
        let mut certs = MbedtlsList::<Certificate>::new();
        let mut pem = pem.into();

        // Null terminate for Mbedtls
        pem.push('\0');
        let cert = Certificate::from_pem(pem.as_bytes())?;
        certs.push(cert);
        Ok(Self(certs))
    }

    /// Try to get a trust anchor from DER encoded bytes.
    ///
    /// # Errors
    /// `Error::MbedTls` if the bytes are not a valid DER certificate.
    pub fn try_from_der(der: impl AsRef<[u8]>) -> Result<Self> {
        let mut certs = MbedtlsList::<Certificate>::new();
        let cert = Certificate::from_der(der.as_ref())?;
        certs.push(cert);
        Ok(Self(certs))
    }
}

/// An unverified certificate chain.
///
/// This is mostly opaque meant to be used to verify and create a
/// [`VerifiedCertChain`].
#[derive(Clone)]
pub struct UnverifiedCertChain(MbedtlsList<Certificate>);

impl Debug for UnverifiedCertChain {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f, "UnverifiedCertChain{{...}}")
    }
}

impl UnverifiedCertChain {
    /// Verify the certificate chain is valid for the given `trust_anchor`.
    ///
    /// # Errors
    /// `Error::MbedTls` if the certificate chain is not valid.
    pub fn verify(
        self,
        trust_anchor: &TrustAnchor,
        mut crl: CertificateRevocationList,
    ) -> Result<VerifiedCertChain> {
        let profile = Profile::new(
            vec![HashType::Sha256, HashType::Sha384, HashType::Sha512],
            // The note on `PkType::Ecdsa` is a lie:
            //
            // > This type is never returned by the mbedTLS key parsing routines
            //
            // It comes back when using the Intel cert chain.
            vec![PkType::Rsa, PkType::Eckey, PkType::Ecdsa],
            vec![
                EcGroupId::Curve25519,
                EcGroupId::SecP256K1,
                EcGroupId::SecP256R1,
                EcGroupId::SecP384R1,
                EcGroupId::SecP521R1,
            ],
            2048,
        );
        Certificate::verify_with_profile(
            &self.0,
            &trust_anchor.0,
            Some(&mut crl.0),
            Some(&profile),
            None,
        )?;
        Ok(VerifiedCertChain(self.0))
    }

    /// Try to get a certificate chain from an iterator of PEM encoded strings
    ///
    /// # Errors
    /// `Error::MbedTls` if one of the strings is not valid PEM certificate(s).
    pub fn try_from_pem<'a, E, I>(pems: I) -> Result<Self>
    where
        I: IntoIterator<Item = &'a E>,
        E: ToString + 'a + ?Sized,
    {
        let mut certs = MbedtlsList::<Certificate>::new();
        for pem in pems.into_iter() {
            let mut pem = pem.to_string();
            // Null terminate for Mbedtls
            pem.push('\0');
            let cert = Certificate::from_pem(pem.as_bytes())?;
            certs.push(cert);
        }
        Ok(Self(certs))
    }

    /// Try to get a certificate chain from an iterator of DER encoded byte
    /// slices.
    ///
    /// # Errors
    /// `Error::MbedTls` if one of the bytes was not valid a DER certificate.
    pub fn try_from_der<E, I>(ders: I) -> Result<Self>
    where
        I: IntoIterator<Item = E>,
        E: AsRef<[u8]>,
    {
        let mut certs = MbedtlsList::<Certificate>::new();
        for der in ders {
            let cert = Certificate::from_der(der.as_ref())?;
            certs.push(cert);
        }
        Ok(Self(certs))
    }
}

/// Certificate revocation list.
#[derive(Debug)]
pub struct CertificateRevocationList(Crl);

impl CertificateRevocationList {
    /// Try to get a set of certificate revocation lists from an iterator of
    /// PEM encoded strings.
    ///
    /// # Errors
    /// `Error::MbedTls` if one of the strings is not a valid PEM CRL.
    pub fn try_from_pem<'a, E, I>(pems: I) -> Result<Self>
    where
        I: IntoIterator<Item = &'a E>,
        E: ToString + 'a + ?Sized,
    {
        let mut crl = Crl::new();
        for pem in pems.into_iter() {
            let mut pem = pem.to_string();
            // Null terminate for Mbedtls
            pem.push('\0');
            crl.push_from_pem(pem.as_bytes())?;
        }
        Ok(Self(crl))
    }

    /// Try to get a set of certificate revocation lists from an iterator of
    /// DER encoded byte slices.
    ///
    /// # Errors
    /// `Error::MbedTls` if one of the slices is not a valid DER CRL.
    pub fn try_from_der<E, I>(ders: I) -> Result<Self>
    where
        I: IntoIterator<Item = E>,
        E: AsRef<[u8]>,
    {
        let mut crl = Crl::new();
        for der in ders {
            crl.push_from_der(der.as_ref())?;
        }
        Ok(Self(crl))
    }
}

/// A verified certificate chain.
///
/// See [`UnverifiedCertChain::verify`] for creating one.
pub struct VerifiedCertChain(MbedtlsList<Certificate>);

impl VerifiedCertChain {
    /// Get the leaf certificate.
    ///
    /// Returns `None` if the chain is empty or only contains CA certificates.
    ///
    /// # Panics
    /// If the leaf certificate is not valid DER. The leaf certificate was
    /// loaded via an [`UnverifiedCertChain`] so it should be valid DER.
    pub fn leaf(&self) -> Option<X509Certificate> {
        for cert in self.0.iter() {
            // Per [rfc5280](https://datatracker.ietf.org/doc/html/rfc5280)
            // The `keyCertSign` bit is asserted for CA certificates, or non
            // leaf certificates.
            if cert.check_key_usage(KeyUsage::KEY_CERT_SIGN) {
                continue;
            }
            return Some(X509Certificate::from_der(cert.as_der()).expect(
                "Failed to parse leaf certificate that was able to load into a certificate chain",
            ));
        }
        None
    }
}

impl Debug for VerifiedCertChain {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f, "VerifiedCertChain{{...}}")
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use x509_cert::der::DecodePem;
    use yare::parameterized;

    const LEAF_CERT: &str = include_str!("../data/tests/leaf_cert.pem");
    const PROCESSOR_CA: &str = include_str!("../data/tests/processor_ca.pem");
    const ROOT_CA: &str = include_str!("../data/tests/root_ca.pem");
    const PROCESSOR_CRL: &str = include_str!("../data/tests/processor_crl.pem");
    const ROOT_CRL: &str = include_str!("../data/tests/root_crl.pem");

    // common PKITs tests data
    const TRUST_ANCHOR_ROOT_CERTIFICATE: &[u8] =
        include_bytes!("../data/tests/pkits/certs/TrustAnchorRootCertificate.crt");
    const TRUST_ANCHOR_ROOT_CRL: &[u8] =
        include_bytes!("../data/tests/pkits/crls/TrustAnchorRootCRL.crl");
    const GOOD_CA_CERT: &[u8] = include_bytes!("../data/tests/pkits/certs/GoodCACert.crt");
    const GOOD_CA_CRL: &[u8] = include_bytes!("../data/tests/pkits/crls/GoodCACRL.crl");

    #[test]
    fn trust_anchor_from_pem() {
        assert!(TrustAnchor::try_from_pem(ROOT_CA).is_ok());
    }

    #[test]
    fn trust_anchor_from_bad_pem_fails() {
        assert!(matches!(
            TrustAnchor::try_from_pem(&ROOT_CA[1..]),
            Err(Error::MbedTls(_))
        ));
    }

    #[test]
    fn trust_anchor_from_der() {
        assert!(TrustAnchor::try_from_der(&TRUST_ANCHOR_ROOT_CERTIFICATE).is_ok());
    }

    #[test]
    fn trust_anchor_from_bad_der_fails() {
        assert!(matches!(
            TrustAnchor::try_from_der(&TRUST_ANCHOR_ROOT_CERTIFICATE[1..]),
            Err(Error::MbedTls(_))
        ));
    }

    #[test]
    fn cert_chain_from_one_pem_cert() {
        let cert_chain =
            UnverifiedCertChain::try_from_pem([LEAF_CERT]).expect("failed to parse cert chain");
        // Counting manually, because `MbedtlsList` is a linked list without
        // `len()` method.
        let count = cert_chain.0.iter().count();
        assert_eq!(count, 1);
    }

    #[test]
    fn cert_chain_from_two_pem_certs() {
        let cert_chain = UnverifiedCertChain::try_from_pem([LEAF_CERT, PROCESSOR_CA])
            .expect("failed to parse cert chain");
        let count = cert_chain.0.iter().count();
        assert_eq!(count, 2);
    }

    #[test]
    fn cert_chain_from_invalid_pem_cert() {
        assert!(matches!(
            UnverifiedCertChain::try_from_pem([&LEAF_CERT[1..]]),
            Err(Error::MbedTls(_))
        ));
    }

    #[test]
    fn cert_chain_from_one_der_cert() {
        let cert_chain = UnverifiedCertChain::try_from_der([TRUST_ANCHOR_ROOT_CERTIFICATE])
            .expect("failed to parse cert chain");
        let count = cert_chain.0.iter().count();
        assert_eq!(count, 1);
    }

    #[test]
    fn cert_chain_from_multiple_der_certs() {
        let cert_chain =
            UnverifiedCertChain::try_from_der([GOOD_CA_CERT, TRUST_ANCHOR_ROOT_CERTIFICATE])
                .expect("failed to parse cert chain");
        let count = cert_chain.0.iter().count();
        assert_eq!(count, 2);
    }

    #[test]
    fn cert_chain_from_invalid_der_cert() {
        assert!(matches!(
            UnverifiedCertChain::try_from_der([&TRUST_ANCHOR_ROOT_CERTIFICATE[1..]]),
            Err(Error::MbedTls(_))
        ));
    }

    #[test]
    fn verify_valid_cert_chain() {
        let cert_chain = UnverifiedCertChain::try_from_pem([LEAF_CERT, PROCESSOR_CA, ROOT_CA])
            .expect("failed to parse cert chain");
        let trust_anchor = TrustAnchor::try_from_pem(ROOT_CA).expect("failed to parse root cert");
        let crl = CertificateRevocationList::try_from_pem([ROOT_CRL, PROCESSOR_CRL])
            .expect("failed to parse CRL");
        assert!(cert_chain.verify(&trust_anchor, crl).is_ok());
    }

    #[test]
    fn invalid_cert_chain() {
        let cert_chain = UnverifiedCertChain::try_from_pem([LEAF_CERT, ROOT_CA])
            .expect("failed to parse cert chain");
        let trust_anchor = TrustAnchor::try_from_pem(ROOT_CA).expect("failed to parse root cert");
        let crl = CertificateRevocationList::try_from_pem([ROOT_CRL, PROCESSOR_CRL])
            .expect("failed to parse CRL");
        assert!(matches!(
            cert_chain.verify(&trust_anchor, crl),
            Err(Error::MbedTls(_))
        ));
    }

    #[test]
    fn unordered_cert_chain_succeeds() {
        let cert_chain = UnverifiedCertChain::try_from_pem([PROCESSOR_CA, ROOT_CA, LEAF_CERT])
            .expect("failed to parse cert chain");
        let trust_anchor = TrustAnchor::try_from_pem(ROOT_CA).expect("failed to parse root cert");
        let crl = CertificateRevocationList::try_from_pem([ROOT_CRL, PROCESSOR_CRL])
            .expect("failed to parse CRL");
        assert!(cert_chain.verify(&trust_anchor, crl).is_ok());
    }

    // The below tests are from the
    // [Public Key Infrastructure Test Suite)[https://csrc.nist.gov/projects/pki-testing]
    // also known as PKITS. The numbers in the test names are the sections in
    // the test description document,
    // <https://csrc.nist.gov/CSRC/media/Projects/PKI-Testing/documents/PKITS.pdf>
    //
    // The point of the tests are to show that we've correctly hooked up CRL
    // logic with mbedtls, not to test out mbedtls with respect to PKITS.
    #[test]
    fn missing_crl_4_4_1() {
        const NO_CRL_CA_CERT: &[u8] = include_bytes!("../data/tests/pkits/certs/NoCRLCACert.crt");
        const INVALID_MISSING_CRL_TEST1_EE: &[u8] =
            include_bytes!("../data/tests/pkits/certs/InvalidMissingCRLTest1EE.crt");
        let ders = [
            INVALID_MISSING_CRL_TEST1_EE,
            NO_CRL_CA_CERT,
            TRUST_ANCHOR_ROOT_CERTIFICATE,
        ];
        let cert_chain =
            UnverifiedCertChain::try_from_der(&ders).expect("failed to parse cert chain");
        let trust_anchor = TrustAnchor::try_from_der(TRUST_ANCHOR_ROOT_CERTIFICATE)
            .expect("failed to parse root cert");
        let crl = CertificateRevocationList::try_from_der([TRUST_ANCHOR_ROOT_CRL])
            .expect("failed to parse CRL");

        // As the name suggests, this test should fail, however Mbedtls doesn't
        // seem to conform to the RFC,
        // <https://datatracker.ietf.org/doc/html/rfc5280#section-6.3.3>
        // It's a bit hard to parse in the RFC, but when CRLs are the
        // invalidation mechanism there should be a CRL for every CA.
        //
        // > After processing such CRLs, if the revocation status has
        // > still not been determined, then return the cert_status
        // > UNDETERMINED.
        //
        // It looks like mbedtls will ignore the missing CRL and continue,
        // <https://github.com/mobilecoinfoundation/rust-mbedtls/blob/6d8fe323a3292f87a6bce4b35963d47139a583f9/mbedtls-sys/vendor/library/x509_crt.c#L2337>
        //
        // > Skip validation if no CRL for the given CA is present.
        assert_eq!(cert_chain.verify(&trust_anchor, crl).is_ok(), true);
    }

    #[test]
    fn invalid_revoked_ca_4_4_2() {
        const REVOKED_SUB_CA_CERT: &[u8] =
            include_bytes!("../data/tests/pkits/certs/RevokedsubCACert.crt");
        const INVALID_REVOKED_CA_TEST2_EE: &[u8] =
            include_bytes!("../data/tests/pkits/certs/InvalidRevokedCATest2EE.crt");
        const REVOKED_SUB_CA_CRL: &[u8] =
            include_bytes!("../data/tests/pkits/crls/RevokedsubCACRL.crl");
        let ders = [
            INVALID_REVOKED_CA_TEST2_EE,
            REVOKED_SUB_CA_CERT,
            GOOD_CA_CERT,
            TRUST_ANCHOR_ROOT_CERTIFICATE,
        ];
        let cert_chain =
            UnverifiedCertChain::try_from_der(&ders).expect("failed to parse cert chain");
        let trust_anchor = TrustAnchor::try_from_der(TRUST_ANCHOR_ROOT_CERTIFICATE)
            .expect("failed to parse root cert");
        let crl = CertificateRevocationList::try_from_der([
            REVOKED_SUB_CA_CRL,
            GOOD_CA_CRL,
            TRUST_ANCHOR_ROOT_CRL,
        ])
        .expect("failed to parse CRL");

        assert!(matches!(
            cert_chain.verify(&trust_anchor, crl),
            Err(Error::MbedTls(_))
        ));
    }

    #[test]
    fn invalid_revoked_ee_4_4_3() {
        const INVALID_REVOKED_EE_TEST3_EE: &[u8] =
            include_bytes!("../data/tests/pkits/certs/InvalidRevokedEETest3EE.crt");
        let ders = [
            INVALID_REVOKED_EE_TEST3_EE,
            GOOD_CA_CERT,
            TRUST_ANCHOR_ROOT_CERTIFICATE,
        ];
        let cert_chain =
            UnverifiedCertChain::try_from_der(&ders).expect("failed to parse cert chain");
        let trust_anchor = TrustAnchor::try_from_der(TRUST_ANCHOR_ROOT_CERTIFICATE)
            .expect("failed to parse root cert");
        let crl = CertificateRevocationList::try_from_der([GOOD_CA_CRL, TRUST_ANCHOR_ROOT_CRL])
            .expect("failed to parse CRL");

        assert!(matches!(
            cert_chain.verify(&trust_anchor, crl),
            Err(Error::MbedTls(_))
        ));
    }

    #[test]
    fn invalid_bad_crl_signature_4_4_4() {
        const BAD_CRL_SIGNATURE_CA_CRL: &[u8] =
            include_bytes!("../data/tests/pkits/crls/BadCRLSignatureCACRL.crl");
        // The CRL signature is invalid so parsing the CRL will fail
        assert!(matches!(
            CertificateRevocationList::try_from_der([
                BAD_CRL_SIGNATURE_CA_CRL,
                TRUST_ANCHOR_ROOT_CRL
            ]),
            Err(Error::MbedTls(_))
        ));
    }

    #[test]
    fn no_leaf_certificate_available() {
        let cert_chain = UnverifiedCertChain::try_from_pem([PROCESSOR_CA, ROOT_CA])
            .expect("failed to parse cert chain");
        let trust_anchor = TrustAnchor::try_from_pem(ROOT_CA).expect("failed to parse root cert");
        let crl = CertificateRevocationList::try_from_pem([ROOT_CRL]).expect("failed to parse CRL");

        let verified_cert_chain = cert_chain
            .verify(&trust_anchor, crl)
            .expect("failed to verify cert chain");

        assert_eq!(verified_cert_chain.leaf(), None);
    }

    #[parameterized(
        first = { &[LEAF_CERT, PROCESSOR_CA, ROOT_CA] },
        middle = { &[PROCESSOR_CA, LEAF_CERT, ROOT_CA] },
        last = { &[ROOT_CA, PROCESSOR_CA, LEAF_CERT] },
    )]
    fn leaf_certificate(pems: &[&str]) {
        let cert_chain =
            UnverifiedCertChain::try_from_pem(pems).expect("failed to parse cert chain");
        let trust_anchor = TrustAnchor::try_from_pem(ROOT_CA).expect("failed to parse root cert");
        let crl = CertificateRevocationList::try_from_pem([ROOT_CRL]).expect("failed to parse CRL");

        let verified_cert_chain = cert_chain
            .verify(&trust_anchor, crl)
            .expect("failed to verify cert chain");

        let expected_certificate =
            X509Certificate::from_pem(LEAF_CERT).expect("failed to parse PEM");
        assert_eq!(verified_cert_chain.leaf(), Some(expected_certificate));
    }
}
