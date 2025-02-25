// Licensed to the Apache Software Foundation (ASF) under one
// or more contributor license agreements.  See the NOTICE file
// distributed with this work for additional information
// regarding copyright ownership.  The ASF licenses this file
// to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance
// with the License.  You may obtain a copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing,
// software distributed under the License is distributed on an
// "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.  See the License for the
// specific language governing permissions and limitations
// under the License.

use std::collections::HashMap;
use std::fmt::Debug;
use std::fmt::Formatter;
use std::sync::Arc;

use async_trait::async_trait;
use http::header::CONTENT_TYPE;
use http::StatusCode;
use log::debug;
use reqsign::AzureStorageConfig;
use reqsign::AzureStorageLoader;
use reqsign::AzureStorageSigner;

use super::batch::parse_batch_delete_response;
use super::error::parse_error;
use super::pager::AzblobPager;
use super::writer::AzblobWriter;
use crate::ops::*;
use crate::raw::*;
use crate::services::azblob::core::AzblobCore;
use crate::types::Metadata;
use crate::*;

/// Known endpoint suffix Azure Storage Blob services resource URI syntax.
/// Azure public cloud: https://accountname.blob.core.windows.net
/// Azure US Government: https://accountname.blob.core.usgovcloudapi.net
/// Azure China: https://accountname.blob.core.chinacloudapi.cn
const KNOWN_AZBLOB_ENDPOINT_SUFFIX: &[&str] = &[
    "blob.core.windows.net",
    "blob.core.usgovcloudapi.net",
    "blob.core.chinacloudapi.cn",
];

const AZBLOB_BATCH_LIMIT: usize = 256;

/// Azure Storage Blob services support.
///
/// # Capabilities
///
/// This service can be used to:
///
/// - [x] read
/// - [x] write
/// - [x] copy
/// - [x] list
/// - [x] scan
/// - [ ] presign
/// - [ ] blocking
///
/// # Configuration
///
/// - `root`: Set the work dir for backend.
/// - `container`: Set the container name for backend.
/// - `endpoint`: Set the endpoint for backend.
/// - `account_name`: Set the account_name for backend.
/// - `account_key`: Set the account_key for backend.
///
/// Refer to public API docs for more information.
///
/// # Example
///
/// This example works on [Azurite](https://github.com/Azure/Azurite) for local developments.
///
/// ## Start local blob service
///
/// ```shell
/// docker run -p 10000:10000 mcr.microsoft.com/azure-storage/azurite
/// az storage container create --name test --connection-string "DefaultEndpointsProtocol=http;AccountName=devstoreaccount1;AccountKey=Eby8vdM02xNOcqFlqUwJPLlmEtlCDXJ1OUzFT50uSRZ6IFsuFq2UVErCz4I6tq/K1SZFPTOtr/KBHBeksoGMGw==;BlobEndpoint=http://127.0.0.1:10000/devstoreaccount1;"
/// ```
///
/// ## Init OpenDAL Operator
///
/// ### Via Builder
///
/// ```no_run
/// use std::sync::Arc;
///
/// use anyhow::Result;
/// use opendal::services::Azblob;
/// use opendal::Operator;
///
/// #[tokio::main]
/// async fn main() -> Result<()> {
///     // Create azblob backend builder.
///     let mut builder = Azblob::default();
///     // Set the root for azblob, all operations will happen under this root.
///     //
///     // NOTE: the root must be absolute path.
///     builder.root("/path/to/dir");
///     // Set the container name, this is required.
///     builder.container("test");
///     // Set the endpoint, this is required.
///     //
///     // For examples:
///     // - "http://127.0.0.1:10000/devstoreaccount1"
///     // - "https://accountname.blob.core.windows.net"
///     builder.endpoint("http://127.0.0.1:10000/devstoreaccount1");
///     // Set the account_name and account_key.
///     //
///     // OpenDAL will try load credential from the env.
///     // If credential not set and no valid credential in env, OpenDAL will
///     // send request without signing like anonymous user.
///     builder.account_name("devstoreaccount1");
///     builder.account_key("Eby8vdM02xNOcqFlqUwJPLlmEtlCDXJ1OUzFT50uSRZ6IFsuFq2UVErCz4I6tq/K1SZFPTOtr/KBHBeksoGMGw==");
///
///     // `Accessor` provides the low level APIs, we will use `Operator` normally.
///     let op: Operator = Operator::new(builder)?.finish();
///
///     Ok(())
/// }
/// ```
#[derive(Default, Clone)]
pub struct AzblobBuilder {
    root: Option<String>,
    container: String,
    endpoint: Option<String>,
    account_name: Option<String>,
    account_key: Option<String>,
    sas_token: Option<String>,
    http_client: Option<HttpClient>,
}

impl Debug for AzblobBuilder {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut ds = f.debug_struct("Builder");

        ds.field("root", &self.root);
        ds.field("container", &self.container);
        ds.field("endpoint", &self.endpoint);

        if self.account_name.is_some() {
            ds.field("account_name", &"<redacted>");
        }
        if self.account_key.is_some() {
            ds.field("account_key", &"<redacted>");
        }
        if self.sas_token.is_some() {
            ds.field("sas_token", &"<redacted>");
        }

        ds.finish()
    }
}

impl AzblobBuilder {
    /// Set root of this backend.
    ///
    /// All operations will happen under this root.
    pub fn root(&mut self, root: &str) -> &mut Self {
        if !root.is_empty() {
            self.root = Some(root.to_string())
        }

        self
    }

    /// Set container name of this backend.
    pub fn container(&mut self, container: &str) -> &mut Self {
        self.container = container.to_string();

        self
    }

    /// Set endpoint of this backend.
    ///
    /// Endpoint must be full uri, e.g.
    ///
    /// - Azblob: `https://accountname.blob.core.windows.net`
    /// - Azurite: `http://127.0.0.1:10000/devstoreaccount1`
    pub fn endpoint(&mut self, endpoint: &str) -> &mut Self {
        if !endpoint.is_empty() {
            // Trim trailing `/` so that we can accept `http://127.0.0.1:9000/`
            self.endpoint = Some(endpoint.trim_end_matches('/').to_string());
        }

        self
    }

    /// Set account_name of this backend.
    ///
    /// - If account_name is set, we will take user's input first.
    /// - If not, we will try to load it from environment.
    pub fn account_name(&mut self, account_name: &str) -> &mut Self {
        if !account_name.is_empty() {
            self.account_name = Some(account_name.to_string());
        }

        self
    }

    /// Set account_key of this backend.
    ///
    /// - If account_key is set, we will take user's input first.
    /// - If not, we will try to load it from environment.
    pub fn account_key(&mut self, account_key: &str) -> &mut Self {
        if !account_key.is_empty() {
            self.account_key = Some(account_key.to_string());
        }

        self
    }

    /// Set sas_token of this backend.
    ///
    /// - If sas_token is set, we will take user's input first.
    /// - If not, we will try to load it from environment.
    ///
    /// See [Grant limited access to Azure Storage resources using shared access signatures (SAS)](https://learn.microsoft.com/en-us/azure/storage/common/storage-sas-overview)
    /// for more info.
    pub fn sas_token(&mut self, sas_token: &str) -> &mut Self {
        if !sas_token.is_empty() {
            self.sas_token = Some(sas_token.to_string());
        }

        self
    }

    /// Specify the http client that used by this service.
    ///
    /// # Notes
    ///
    /// This API is part of OpenDAL's Raw API. `HttpClient` could be changed
    /// during minor updates.
    pub fn http_client(&mut self, client: HttpClient) -> &mut Self {
        self.http_client = Some(client);
        self
    }

    /// from_connection_string will make a builder from connection string
    ///
    /// connection string looks like:
    ///
    /// ```txt
    /// DefaultEndpointsProtocol=http;AccountName=devstoreaccount1;
    /// AccountKey=Eby8vdM02xNOcqFlqUwJPLlmEtlCDXJ1OUzFT50uSRZ6IFsuFq2UVErCz4I6tq/K1SZFPTOtr/KBHBeksoGMGw==;
    /// BlobEndpoint=http://127.0.0.1:10000/devstoreaccount1;
    /// QueueEndpoint=http://127.0.0.1:10001/devstoreaccount1;
    /// TableEndpoint=http://127.0.0.1:10002/devstoreaccount1;
    /// ```
    ///
    /// Or
    ///
    /// ```txt
    /// DefaultEndpointsProtocol=https;
    /// AccountName=storagesample;
    /// AccountKey=<account-key>;
    /// EndpointSuffix=core.chinacloudapi.cn;
    /// ```
    ///
    /// For reference: [Configure Azure Storage connection strings](https://learn.microsoft.com/en-us/azure/storage/common/storage-configure-connection-string)
    ///
    /// # Note
    ///
    /// connection string only configures the endpoint, account name and account key.
    /// User still needs to configure bucket names.
    pub fn from_connection_string(conn: &str) -> Result<Self> {
        let conn = conn.trim().replace('\n', "");

        let mut conn_map: HashMap<_, _> = HashMap::default();
        for v in conn.split(';') {
            let entry: Vec<_> = v.splitn(2, '=').collect();
            if entry.len() != 2 {
                // Ignore invalid entries.
                continue;
            }
            conn_map.insert(entry[0], entry[1]);
        }

        let mut builder = AzblobBuilder::default();

        if let Some(sas_token) = conn_map.get("SharedAccessSignature") {
            builder.sas_token(sas_token);
        } else {
            let account_name = conn_map.get("AccountName").ok_or_else(|| {
                Error::new(
                    ErrorKind::ConfigInvalid,
                    "connection string must have AccountName",
                )
                .with_operation("Builder::from_connection_string")
            })?;
            builder.account_name(account_name);
            let account_key = conn_map.get("AccountKey").ok_or_else(|| {
                Error::new(
                    ErrorKind::ConfigInvalid,
                    "connection string must have AccountKey",
                )
                .with_operation("Builder::from_connection_string")
            })?;
            builder.account_key(account_key);
        }

        if let Some(v) = conn_map.get("BlobEndpoint") {
            builder.endpoint(v);
        } else if let Some(v) = conn_map.get("EndpointSuffix") {
            let protocol = conn_map.get("DefaultEndpointsProtocol").unwrap_or(&"https");
            let account_name = builder
                .account_name
                .as_ref()
                .ok_or_else(|| {
                    Error::new(
                        ErrorKind::ConfigInvalid,
                        "connection string must have AccountName",
                    )
                    .with_operation("Builder::from_connection_string")
                })?
                .clone();
            builder.endpoint(&format!("{protocol}://{account_name}.blob.{v}"));
        }

        Ok(builder)
    }
}

impl Builder for AzblobBuilder {
    const SCHEME: Scheme = Scheme::Azblob;
    type Accessor = AzblobBackend;

    fn from_map(map: HashMap<String, String>) -> Self {
        let mut builder = AzblobBuilder::default();

        map.get("root").map(|v| builder.root(v));
        map.get("container").map(|v| builder.container(v));
        map.get("endpoint").map(|v| builder.endpoint(v));
        map.get("account_name").map(|v| builder.account_name(v));
        map.get("account_key").map(|v| builder.account_key(v));
        map.get("sas_token").map(|v| builder.sas_token(v));

        builder
    }

    fn build(&mut self) -> Result<Self::Accessor> {
        debug!("backend build started: {:?}", &self);

        let root = normalize_root(&self.root.take().unwrap_or_default());
        debug!("backend use root {}", root);

        // Handle endpoint, region and container name.
        let container = match self.container.is_empty() {
            false => Ok(&self.container),
            true => Err(Error::new(ErrorKind::ConfigInvalid, "container is empty")
                .with_operation("Builder::build")
                .with_context("service", Scheme::Azblob)),
        }?;
        debug!("backend use container {}", &container);

        let endpoint = match &self.endpoint {
            Some(endpoint) => Ok(endpoint.clone()),
            None => Err(Error::new(ErrorKind::ConfigInvalid, "endpoint is empty")
                .with_operation("Builder::build")
                .with_context("service", Scheme::Azblob)),
        }?;
        debug!("backend use endpoint {}", &container);

        let client = if let Some(client) = self.http_client.take() {
            client
        } else {
            HttpClient::new().map_err(|err| {
                err.with_operation("Builder::build")
                    .with_context("service", Scheme::Azblob)
            })?
        };

        let config_loader = AzureStorageConfig {
            account_name: self
                .account_name
                .clone()
                .or_else(|| infer_storage_name_from_endpoint(endpoint.as_str())),
            account_key: self.account_key.clone(),
            sas_token: self.sas_token.clone(),
        };

        let cred_loader = AzureStorageLoader::new(config_loader);

        let signer = AzureStorageSigner::new();
        let batch_signer = AzureStorageSigner::new().omit_service_version();

        debug!("backend build finished: {:?}", &self);
        Ok(AzblobBackend {
            core: Arc::new(AzblobCore {
                root,
                endpoint,
                container: self.container.clone(),

                client,
                loader: cred_loader,
                signer,
                batch_signer,
            }),
        })
    }
}

fn infer_storage_name_from_endpoint(endpoint: &str) -> Option<String> {
    let endpoint: &str = endpoint
        .strip_prefix("http://")
        .or_else(|| endpoint.strip_prefix("https://"))
        .unwrap_or(endpoint);

    let mut parts = endpoint.splitn(2, '.');
    let storage_name = parts.next();
    let endpoint_suffix = parts
        .next()
        .unwrap_or_default()
        .trim_end_matches('/')
        .to_lowercase();

    if KNOWN_AZBLOB_ENDPOINT_SUFFIX
        .iter()
        .any(|s| *s == endpoint_suffix.as_str())
    {
        storage_name.map(|s| s.to_string())
    } else {
        None
    }
}

/// Backend for azblob services.
#[derive(Debug, Clone)]
pub struct AzblobBackend {
    core: Arc<AzblobCore>,
}

#[async_trait]
impl Accessor for AzblobBackend {
    type Reader = IncomingAsyncBody;
    type BlockingReader = ();
    type Writer = AzblobWriter;
    type BlockingWriter = ();
    type Pager = AzblobPager;
    type BlockingPager = ();

    fn info(&self) -> AccessorInfo {
        use AccessorCapability::*;
        use AccessorHint::*;

        let mut am = AccessorInfo::default();
        am.set_scheme(Scheme::Azblob)
            .set_root(&self.core.root)
            .set_name(&self.core.container)
            .set_max_batch_operations(AZBLOB_BATCH_LIMIT)
            .set_capabilities(Read | Write | List | Scan | Batch | Copy)
            .set_hints(ReadStreamable);

        am
    }

    async fn create_dir(&self, path: &str, _: OpCreate) -> Result<RpCreate> {
        let mut req = self
            .core
            .azblob_put_blob_request(path, Some(0), None, AsyncBody::Empty)?;

        self.core.sign(&mut req).await?;

        let resp = self.core.send(req).await?;

        let status = resp.status();

        match status {
            StatusCode::CREATED | StatusCode::OK => {
                resp.into_body().consume().await?;
                Ok(RpCreate::default())
            }
            _ => Err(parse_error(resp).await?),
        }
    }

    async fn read(&self, path: &str, args: OpRead) -> Result<(RpRead, Self::Reader)> {
        let resp = self.core.azblob_get_blob(path, args.range()).await?;

        let status = resp.status();

        match status {
            StatusCode::OK | StatusCode::PARTIAL_CONTENT => {
                let meta = parse_into_metadata(path, resp.headers())?;

                Ok((RpRead::with_metadata(meta), resp.into_body()))
            }
            _ => Err(parse_error(resp).await?),
        }
    }

    async fn write(&self, path: &str, args: OpWrite) -> Result<(RpWrite, Self::Writer)> {
        if args.append() {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "append write is not supported",
            ));
        }

        Ok((
            RpWrite::default(),
            AzblobWriter::new(self.core.clone(), args, path.to_string()),
        ))
    }

    async fn copy(&self, from: &str, to: &str, _args: OpCopy) -> Result<RpCopy> {
        let resp = self.core.azblob_copy_blob(from, to).await?;

        let status = resp.status();

        match status {
            StatusCode::ACCEPTED => {
                resp.into_body().consume().await?;
                Ok(RpCopy::default())
            }
            _ => Err(parse_error(resp).await?),
        }
    }

    async fn stat(&self, path: &str, _: OpStat) -> Result<RpStat> {
        // Stat root always returns a DIR.
        if path == "/" {
            return Ok(RpStat::new(Metadata::new(EntryMode::DIR)));
        }

        let resp = self.core.azblob_get_blob_properties(path).await?;

        let status = resp.status();

        match status {
            StatusCode::OK => parse_into_metadata(path, resp.headers()).map(RpStat::new),
            StatusCode::NOT_FOUND if path.ends_with('/') => {
                Ok(RpStat::new(Metadata::new(EntryMode::DIR)))
            }
            _ => Err(parse_error(resp).await?),
        }
    }

    async fn delete(&self, path: &str, _: OpDelete) -> Result<RpDelete> {
        let resp = self.core.azblob_delete_blob(path).await?;

        let status = resp.status();

        match status {
            StatusCode::ACCEPTED | StatusCode::NOT_FOUND => Ok(RpDelete::default()),
            _ => Err(parse_error(resp).await?),
        }
    }

    async fn list(&self, path: &str, args: OpList) -> Result<(RpList, Self::Pager)> {
        let op = AzblobPager::new(
            self.core.clone(),
            path.to_string(),
            "/".to_string(),
            args.limit(),
        );

        Ok((RpList::default(), op))
    }

    async fn scan(&self, path: &str, args: OpScan) -> Result<(RpScan, Self::Pager)> {
        let op = AzblobPager::new(
            self.core.clone(),
            path.to_string(),
            "".to_string(),
            args.limit(),
        );

        Ok((RpScan::default(), op))
    }

    async fn batch(&self, args: OpBatch) -> Result<RpBatch> {
        let ops = args.into_operation();
        let paths = ops.into_iter().map(|(p, _)| p).collect::<Vec<_>>();
        if paths.len() > AZBLOB_BATCH_LIMIT {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "batch delete limit exceeded",
            ));
        }
        // construct and complete batch request
        let resp = self.core.azblob_batch_delete(&paths).await?;

        // check response status
        if resp.status() != StatusCode::ACCEPTED {
            return Err(parse_error(resp).await?);
        }

        // get boundary from response header
        let content_type = resp.headers().get(CONTENT_TYPE).ok_or_else(|| {
            Error::new(
                ErrorKind::Unexpected,
                "response data should have CONTENT_TYPE header",
            )
        })?;
        let content_type = content_type
            .to_str()
            .map(|ty| ty.to_string())
            .map_err(|e| {
                Error::new(
                    ErrorKind::Unexpected,
                    &format!("get invalid CONTENT_TYPE header in response: {:?}", e),
                )
            })?;
        let splits = content_type.split("boundary=").collect::<Vec<&str>>();
        let boundary = splits.get(1).to_owned().ok_or_else(|| {
            Error::new(
                ErrorKind::Unexpected,
                "No boundary message provided in CONTENT_TYPE",
            )
        })?;

        let body = resp.into_body().bytes().await?;
        let body = String::from_utf8(body.to_vec()).map_err(|e| {
            Error::new(
                ErrorKind::Unexpected,
                &format!("get invalid batch response {e:?}"),
            )
        })?;

        let results = parse_batch_delete_response(boundary, body, paths)?
            .into_iter()
            .map(|(path, rp)| (path, rp.map(|v| v.into())))
            .collect();
        Ok(RpBatch::new(results))
    }
}

#[cfg(test)]
mod tests {
    use super::AzblobBuilder;
    use crate::services::azblob::backend::infer_storage_name_from_endpoint;
    use crate::Builder;

    #[test]
    fn test_infer_storage_name_from_endpoint() {
        let endpoint = "https://account.blob.core.windows.net";
        let storage_name = infer_storage_name_from_endpoint(endpoint);
        assert_eq!(storage_name, Some("account".to_string()));
    }

    #[test]
    fn test_infer_storage_name_from_endpoint_with_trailing_slash() {
        let endpoint = "https://account.blob.core.windows.net/";
        let storage_name = infer_storage_name_from_endpoint(endpoint);
        assert_eq!(storage_name, Some("account".to_string()));
    }

    #[test]
    fn test_builder_from_endpoint_and_key_infer_account_name() {
        let mut azblob_builder = AzblobBuilder::default();
        azblob_builder.endpoint("https://storagesample.blob.core.chinacloudapi.cn");
        azblob_builder.container("container");
        azblob_builder.account_key("account-key");
        let azblob = azblob_builder
            .build()
            .expect("build azblob should be succeeded.");

        assert_eq!(
            azblob.core.endpoint,
            "https://storagesample.blob.core.chinacloudapi.cn"
        );

        assert_eq!(azblob.core.container, "container".to_string());

        assert_eq!(
            azblob_builder.account_key.unwrap(),
            "account-key".to_string()
        );
    }

    #[test]
    fn test_no_key_wont_infer_account_name() {
        let mut azblob_builder = AzblobBuilder::default();
        azblob_builder.endpoint("https://storagesample.blob.core.windows.net");
        azblob_builder.container("container");
        let azblob = azblob_builder
            .build()
            .expect("build azblob should be succeeded.");

        assert_eq!(
            azblob.core.endpoint,
            "https://storagesample.blob.core.windows.net"
        );

        assert_eq!(azblob.core.container, "container".to_string());

        assert_eq!(azblob_builder.account_key, None);
    }

    #[test]
    fn test_builder_from_endpoint_and_sas() {
        let mut azblob_builder = AzblobBuilder::default();
        azblob_builder.endpoint("https://storagesample.blob.core.usgovcloudapi.net");
        azblob_builder.container("container");
        azblob_builder.account_name("storagesample");
        azblob_builder.account_key("account-key");
        azblob_builder.sas_token("sas");
        let azblob = azblob_builder
            .build()
            .expect("build azblob should be succeeded.");

        assert_eq!(
            azblob.core.endpoint,
            "https://storagesample.blob.core.usgovcloudapi.net"
        );

        assert_eq!(azblob.core.container, "container".to_string());

        assert_eq!(
            azblob_builder.account_key.unwrap(),
            "account-key".to_string()
        );

        assert_eq!(azblob_builder.sas_token.unwrap(), "sas".to_string());
    }

    #[test]
    fn test_builder_from_connection_string() {
        let builder = AzblobBuilder::from_connection_string(
            r#"
DefaultEndpointsProtocol=http;AccountName=devstoreaccount1;
AccountKey=Eby8vdM02xNOcqFlqUwJPLlmEtlCDXJ1OUzFT50uSRZ6IFsuFq2UVErCz4I6tq/K1SZFPTOtr/KBHBeksoGMGw==;
BlobEndpoint=http://127.0.0.1:10000/devstoreaccount1;
QueueEndpoint=http://127.0.0.1:10001/devstoreaccount1;
TableEndpoint=http://127.0.0.1:10002/devstoreaccount1;
        "#,
        )
        .expect("from connection string must succeed");

        assert_eq!(
            builder.endpoint.unwrap(),
            "http://127.0.0.1:10000/devstoreaccount1"
        );
        assert_eq!(builder.account_name.unwrap(), "devstoreaccount1");
        assert_eq!(builder.account_key.unwrap(), "Eby8vdM02xNOcqFlqUwJPLlmEtlCDXJ1OUzFT50uSRZ6IFsuFq2UVErCz4I6tq/K1SZFPTOtr/KBHBeksoGMGw==");

        let builder = AzblobBuilder::from_connection_string(
            r#"
DefaultEndpointsProtocol=https;
AccountName=storagesample;
AccountKey=account-key;
EndpointSuffix=core.chinacloudapi.cn;
        "#,
        )
        .expect("from connection string must succeed");

        assert_eq!(
            builder.endpoint.unwrap(),
            "https://storagesample.blob.core.chinacloudapi.cn"
        );
        assert_eq!(builder.account_name.unwrap(), "storagesample");
        assert_eq!(builder.account_key.unwrap(), "account-key")
    }

    #[test]
    fn test_sas_from_connection_string() {
        // Note, not a correct HMAC
        let builder = AzblobBuilder::from_connection_string(
            r#"
BlobEndpoint=http://127.0.0.1:10000/devstoreaccount1;
QueueEndpoint=http://127.0.0.1:10001/devstoreaccount1;
TableEndpoint=http://127.0.0.1:10002/devstoreaccount1;
SharedAccessSignature=sv=2021-01-01&ss=b&srt=c&sp=rwdlaciytfx&se=2022-01-01T11:00:14Z&st=2022-01-02T03:00:14Z&spr=https&sig=KEllk4N8f7rJfLjQCmikL2fRVt%2B%2Bl73UBkbgH%2FK3VGE%3D
        "#,
        )
        .expect("from connection string must succeed");

        assert_eq!(
            builder.endpoint.unwrap(),
            "http://127.0.0.1:10000/devstoreaccount1"
        );
        assert_eq!(builder.sas_token.unwrap(), "sv=2021-01-01&ss=b&srt=c&sp=rwdlaciytfx&se=2022-01-01T11:00:14Z&st=2022-01-02T03:00:14Z&spr=https&sig=KEllk4N8f7rJfLjQCmikL2fRVt%2B%2Bl73UBkbgH%2FK3VGE%3D");
        assert_eq!(builder.account_name, None);
        assert_eq!(builder.account_key, None);
    }

    #[test]
    pub fn test_sas_preferred() {
        let builder = AzblobBuilder::from_connection_string(
            r#"
BlobEndpoint=http://127.0.0.1:10000/devstoreaccount1;
AccountName=storagesample;
AccountKey=account-key;
SharedAccessSignature=sv=2021-01-01&ss=b&srt=c&sp=rwdlaciytfx&se=2022-01-01T11:00:14Z&st=2022-01-02T03:00:14Z&spr=https&sig=KEllk4N8f7rJfLjQCmikL2fRVt%2B%2Bl73UBkbgH%2FK3VGE%3D
        "#,
        )
        .expect("from connection string must succeed");

        // SAS should be preferred over shared key
        assert_eq!(builder.sas_token.unwrap(), "sv=2021-01-01&ss=b&srt=c&sp=rwdlaciytfx&se=2022-01-01T11:00:14Z&st=2022-01-02T03:00:14Z&spr=https&sig=KEllk4N8f7rJfLjQCmikL2fRVt%2B%2Bl73UBkbgH%2FK3VGE%3D");
        assert_eq!(builder.account_name, None);
        assert_eq!(builder.account_key, None);
    }
}
