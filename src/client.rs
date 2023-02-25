use aws_config::meta::region::RegionProviderChain;
use aws_sdk_s3::Region;
use chrono::TimeZone;

use crate::app::{FileDetail, Item};

const DELIMITER: &str = "/";
const DEFAULT_REGION: &str = "ap-northeast-1";

pub struct Client {
    pub client: aws_sdk_s3::Client,
}

impl Client {
    pub async fn new(
        region: Option<String>,
        endpoint_url: Option<String>,
        profile: Option<String>,
    ) -> Client {
        let region_provider = RegionProviderChain::first_try(region.map(Region::new))
            .or_default_provider()
            .or_else(DEFAULT_REGION);

        let mut config_loader = aws_config::from_env().region(region_provider);
        if let Some(url) = &endpoint_url {
            config_loader = config_loader.endpoint_url(url);
        }
        if let Some(profile) = &profile {
            config_loader = config_loader.profile_name(profile);
        }
        let sdk_config = config_loader.load().await;

        let mut config_builder = aws_sdk_s3::config::Builder::from(&sdk_config);
        if endpoint_url.is_some() {
            config_builder = config_builder.force_path_style(true);
        }
        let config = config_builder.build();

        let client = aws_sdk_s3::Client::from_conf(config);
        Client { client }
    }

    pub async fn load_all_buckets(&self) -> Vec<Item> {
        let result = self.client.list_buckets().send().await;
        let output = result.unwrap();

        let buckets = output.buckets().unwrap_or_default();
        buckets
            .iter()
            .map(|bucket| {
                let name = bucket.name().unwrap().to_string();
                Item::Bucket { name }
            })
            .collect()
    }

    pub async fn load_objects(&self, bucket: &String, prefix: &String) -> Vec<Item> {
        let result = self
            .client
            .list_objects_v2()
            .bucket(bucket)
            .prefix(prefix)
            .delimiter(DELIMITER)
            .send()
            .await;
        let output = result.unwrap();

        let objects = output.common_prefixes().unwrap_or_default();
        let dirs = objects.iter().map(|dir| {
            let path = dir.prefix().unwrap().to_string();
            let paths = parse_path(&path, true);
            let name = paths.last().unwrap().to_owned();
            Item::Dir { name, paths }
        });

        let objects = output.contents().unwrap_or_default();
        let files = objects.iter().map(|file| {
            let path = file.key().unwrap().to_string();
            let paths = parse_path(&path, false);
            let name = paths.last().unwrap().to_owned();
            let size_byte = file.size();
            let last_modified = convert_datetime(file.last_modified().unwrap());
            Item::File {
                name,
                paths,
                size_byte,
                last_modified,
            }
        });

        dirs.chain(files).collect()
    }

    pub async fn load_object_detail(
        &self,
        bucket: &String,
        key: &String,
        name: &String,
        size_byte: i64,
    ) -> FileDetail {
        let result = self
            .client
            .head_object()
            .bucket(bucket)
            .key(key)
            .send()
            .await;
        let output = result.unwrap();

        let name = name.to_owned();
        let last_modified = convert_datetime(output.last_modified().unwrap());
        let e_tag = output.e_tag().unwrap().trim_matches('"').to_string();
        let content_type = output.content_type().unwrap().to_string();
        FileDetail {
            name,
            size_byte,
            last_modified,
            e_tag,
            content_type,
        }
    }
}

fn parse_path(path: &str, dir: bool) -> Vec<String> {
    let ss: Vec<String> = path.split(DELIMITER).map(|s| s.to_string()).collect();
    if dir {
        let n = ss.len() - 1;
        ss.into_iter().take(n).collect()
    } else {
        ss
    }
}

fn convert_datetime(dt: &aws_smithy_types::DateTime) -> chrono::DateTime<chrono::Local> {
    let nanos = dt.as_nanos();
    chrono::Local.timestamp_nanos(nanos as i64)
}
