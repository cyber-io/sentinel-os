//! TPM manager using tss-esapi

use std::sync::Arc;

use anyhow::Result;
use tss_esapi::{
    constants::tss::*,
    interface_types::{
        algorithm::HashingAlgorithm,
        ecc::EccCurve,
        key_bits::RsaKeyBits,
        resource_handles::Hierarchy,
    },
    structures::{
        Auth, CapabilityData, CreatePrimaryKeyResult, Digest, EccPoint, EccScheme,
        KeyedHashScheme, Public, PublicBuilder, RsaScheme, Sensitivity, TpmtSymDef,
        TpmtSymDefObject, TpmsCapabilityData, TpmtPublic,
    },
    tcti_ldr::TctiNameConf,
    Context, Tcti,
};
use tracing::{debug, info, warn};

use crate::quote::AikHandle;

#[derive(Debug, Clone, serde::Deserialize)]
pub struct TpmConfig {
    pub device: String,
    pub tcti: String,
    pub owner_auth: Option<String>,
    pub endorsement_auth: Option<String>,
    pub lockout_auth: Option<String>,
}

pub struct TpmManager {
    config: TpmConfig,
    context: Option<Context>,
    srk_handle: Option<tss_esapi::handles::KeyHandle>,
    aik_handle: Option<AikHandle>,
}

impl TpmManager {
    pub async fn new(config: &TpmConfig) -> Result<Self> {
        let mut manager = Self {
            config: config.clone(),
            context: None,
            srk_handle: None,
            aik_handle: None,
        };

        manager.initialize().await?;
        Ok(manager)
    }

    async fn initialize(&mut self) -> Result<()> {
        // Create TCTI
        let tcti = Tcti::new_device(&self.config.device, 0)
            .map_err(|e| anyhow::anyhow!("Failed to create TCTI: {}", e))?;

        // Create context
        let mut context = Context::new(tcti)
            .map_err(|e| anyhow::anyhow!("Failed to create TPM context: {}", e))?;

        // Startup TPM
        context.startup(tss_esapi::constants::StartupType::Clear)
            .map_err(|e| anyhow::anyhow!("TPM startup failed: {}", e))?;

        info!("TPM initialized on {}", self.config.device);

        // Create SRK (Storage Root Key)
        self.create_srk(&mut context).await?;

        // Create AIK (Attestation Identity Key)
        self.create_aik(&mut context).await?;

        self.context = Some(context);
        Ok(())
    }

    async fn create_srk(&mut self, context: &mut Context) -> Result<()> {
        let srk_template = PublicBuilder::new()
            .with_public_algorithm(tss_esapi::interface_types::algorithm::PublicAlgorithm::Rsa)
            .with_name_alg(HashingAlgorithm::Sha256)
            .with_object_attributes(
                tss_esapi::structures::ObjectAttributesBuilder::new()
                    .with_fixed_tpm(true)
                    .with_fixed_parent(true)
                    .with_sensitive_data_origin(true)
                    .with_user_with_auth(true)
                    .with_restricted(true)
                    .with_decrypt(true)
                    .build()?
            )
            .with_rsa_parameters(
                tss_esapi::structures::RsaScheme::Null,
                RsaKeyBits::Rsa2048,
                tss_esapi::structures::RsaExponent::default(),
                None,
            )?
            .build()?;

        let srk_auth = Auth::try_from("")?;

        let result = context.create_primary(
            Hierarchy::Owner,
            srk_template,
            None,
            None,
            None,
        )?;

        self.srk_handle = Some(result.key_handle);
        info!("SRK created: {:?}", result.key_handle);
        Ok(())
    }

    async fn create_aik(&mut self, context: &mut Context) -> Result<()> {
        let srk = self.srk_handle.ok_or_else(|| anyhow::anyhow!("SRK not created"))?;

        let aik_template = PublicBuilder::new()
            .with_public_algorithm(tss_esapi::interface_types::algorithm::PublicAlgorithm::Rsa)
            .with_name_alg(HashingAlgorithm::Sha256)
            .with_object_attributes(
                tss_esapi::structures::ObjectAttributesBuilder::new()
                    .with_fixed_tpm(true)
                    .with_fixed_parent(true)
                    .with_sensitive_data_origin(true)
                    .with_user_with_auth(true)
                    .with_restricted(true)
                    .with_sign_encrypt(true)
                    .build()?
            )
            .with_rsa_parameters(
                tss_esapi::structures::RsaScheme::Pkcs1v1_5(HashingAlgorithm::Sha256),
                RsaKeyBits::Rsa2048,
                tss_esapi::structures::RsaExponent::default(),
                None,
            )?
            .build()?;

        let aik_auth = Auth::try_from("")?;

        let result = context.create(
            srk,
            aik_template,
            None,
            None,
            None,
        )?;

        let aik_public = result.out_public;
        let aik_name = aik_public.name()?;

        self.aik_handle = Some(AikHandle {
            handle: result.key_handle,
            public: aik_public,
            name: aik_name,
        });

        info!("AIK created: {:?}", result.key_handle);
        Ok(())
    }

    pub fn get_context(&mut self) -> Result<&mut Context> {
        self.context.as_mut()
            .ok_or_else(|| anyhow::anyhow!("TPM not initialized"))
    }

    pub fn get_srk(&self) -> Result<tss_esapi::handles::KeyHandle> {
        self.srk_handle.ok_or_else(|| anyhow::anyhow!("SRK not available"))
    }

    pub fn get_aik(&self) -> Result<&AikHandle> {
        self.aik_handle.as_ref()
            .ok_or_else(|| anyhow::anyhow!("AIK not available"))
    }

    pub async fn read_pcrs(&mut self, pcrs: &[u32]) -> Result<Vec<Digest>> {
        let context = self.get_context()?;
        let mut results = Vec::new();

        for pcr in pcrs {
            let selection = tss_esapi::structures::PcrSelectionListBuilder::new()
                .with_selection(
                    tss_esapi::interface_types::algorithm::HashingAlgorithm::Sha256,
                    &[*pcr as u8],
                )?
                .build()?;

            let pcr_read = context.pcr_read(selection)?;
            if let Some(digest) = pcr_read.pcr_values.first() {
                results.push(*digest);
            }
        }

        Ok(results)
    }

    pub async fn quote(
        &mut self,
        qualifying_data: &[u8],
        pcrs: &[u32],
    ) -> Result<QuoteResult> {
        let context = self.get_context()?;
        let aik = self.get_aik()?;

        let selection = tss_esapi::structures::PcrSelectionListBuilder::new()
            .with_selection(
                HashingAlgorithm::Sha256,
                &pcrs.iter().map(|&p| p as u8).collect::<Vec<_>>(),
            )?
            .build()?;

        let qualifying_data_digest = Digest::try_from(qualifying_data)?;

        let quote_result = context.quote(
            aik.handle,
            qualifying_data_digest,
            HashingAlgorithm::Sha256,
            selection,
        )?;

        Ok(QuoteResult {
            quote: quote_result.quoted,
            signature: quote_result.signature,
            pcr_values: quote_result.pcr_values,
        })
    }

    pub async fn get_capabilities(&mut self) -> Result<CapabilityData> {
        let context = self.get_context()?;
        Ok(context.get_capability(
            tss_esapi::interface_types::capability::CapabilityType::Properties,
            TpmtCapabilityData::TpmProperties(tss_esapi::structures::TpmtTaggedProperty::default()),
            100,
        )?)
    }

    pub async fn extend_pcr(&mut self, pcr: u32, data: &[u8]) -> Result<()> {
        let context = self.get_context()?;
        let digest = Digest::try_from(data)?;

        context.pcr_extend(pcr as u8, HashingAlgorithm::Sha256, digest)?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct AikHandle {
    pub handle: tss_esapi::handles::KeyHandle,
    pub public: TpmtPublic,
    pub name: tss_esapi::structures::Name,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuoteResult {
    pub quote: tss_esapi::structures::TpmtAttest,
    pub signature: tss_esapi::structures::TpmtSignature,
    pub pcr_values: tss_esapi::structures::PcrSelectionList,
}