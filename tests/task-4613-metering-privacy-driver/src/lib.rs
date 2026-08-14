#[cfg(test)]
mod tests {
    use ipc::{
        crypto_top_up::{
            CryptoCoin, CryptoInvoicePayment, CryptoTopUpInvoice, CryptoTopUpInvoiceProcessor,
        },
        metered_bytes::{MeteredSendPath, MonthlyAllowanceAccount, TopUpPack},
    };
    use serde_json::{json, Value};
    use std::{fs, path::PathBuf};

    const ACCOUNT_RESET: i64 = 1_785_585_600;
    const SETTLED_AT: i64 = 1_786_363_200;
    const MONERO_AMOUNT_ATOMIC: u128 = 1_250_000_000_000;
    const XMR_CONFIRMATIONS: u32 = 10;
    const GRANT_AUDIENCE: &str = "osl-blob-store";
    const BLOB_COUNT: usize = 5;
    const ACCOUNT_FIELDS: [&str; 5] = [
        "account_id",
        "stripe_customer_id",
        "email",
        "public_handle",
        "device_name",
    ];

    #[derive(Debug)]
    struct BuyerIdentity {
        account_id: &'static str,
        stripe_customer_id: &'static str,
        email: &'static str,
        public_handle: &'static str,
        device_name: &'static str,
    }

    impl BuyerIdentity {
        fn values(&self) -> [&str; 5] {
            [
                self.account_id,
                self.stripe_customer_id,
                self.email,
                self.public_handle,
                self.device_name,
            ]
        }
    }

    #[derive(Debug, Default, PartialEq, Eq)]
    struct AccountFieldCounts {
        account_ids: usize,
        stripe_customer_ids: usize,
        emails: usize,
        public_handles: usize,
        device_names: usize,
    }

    impl AccountFieldCounts {
        fn total(&self) -> usize {
            self.account_ids
                + self.stripe_customer_ids
                + self.emails
                + self.public_handles
                + self.device_names
        }
    }

    #[derive(Debug, Default)]
    struct AccountFieldPresence {
        account_id: bool,
        stripe_customer_id: bool,
        email: bool,
        public_handle: bool,
        device_name: bool,
    }

    fn visit_store_value(value: &Value, buyer: &BuyerIdentity, present: &mut AccountFieldPresence) {
        match value {
            Value::Object(fields) => {
                for (field, value) in fields {
                    match field.as_str() {
                        "account_id" => present.account_id = true,
                        "stripe_customer_id" => present.stripe_customer_id = true,
                        "email" => present.email = true,
                        "public_handle" => present.public_handle = true,
                        "device_name" => present.device_name = true,
                        _ => {}
                    }
                    visit_store_value(value, buyer, present);
                }
            }
            Value::Array(values) => {
                for value in values {
                    visit_store_value(value, buyer, present);
                }
            }
            Value::String(text) => {
                if text == buyer.account_id {
                    present.account_id = true;
                }
                if text == buyer.stripe_customer_id {
                    present.stripe_customer_id = true;
                }
                if text == buyer.email {
                    present.email = true;
                }
                if text == buyer.public_handle {
                    present.public_handle = true;
                }
                if text == buyer.device_name {
                    present.device_name = true;
                }
            }
            _ => {}
        }
    }

    fn audit_store(records: &[Value], buyer: &BuyerIdentity) -> AccountFieldCounts {
        let mut counts = AccountFieldCounts::default();
        for record in records {
            assert!(
                record.get("blob_id").is_some(),
                "store record must name its blob"
            );
            let mut present = AccountFieldPresence::default();
            visit_store_value(record, buyer, &mut present);
            counts.account_ids += usize::from(present.account_id);
            counts.stripe_customer_ids += usize::from(present.stripe_customer_id);
            counts.emails += usize::from(present.email);
            counts.public_handles += usize::from(present.public_handle);
            counts.device_names += usize::from(present.device_name);
        }
        counts
    }

    fn grant_claims(index: usize) -> Value {
        json!({
            "aud": GRANT_AUDIENCE,
            "exp": SETTLED_AT + 600,
            "jti": format!("{:032x}", 0xf000_u128 + index as u128),
        })
    }

    fn visible_grant_fields(records: &[Value]) -> Vec<String> {
        let mut fields = records
            .iter()
            .flat_map(|record| {
                record["upload_grant"]
                    .as_object()
                    .expect("store saw a JSON upload grant")
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        fields.sort();
        fields.dedup();
        fields
    }

    fn inject_requested_leak(records: &mut [Value], buyer: &BuyerIdentity) {
        if std::env::var("TASK4613_INJECT_ACCOUNT_FIELD").as_deref() != Ok("account_id") {
            return;
        }
        for record in records {
            record
                .as_object_mut()
                .expect("store record object")
                .insert("account_id".to_owned(), json!(buyer.account_id));
        }
    }

    fn shipping_store_artifacts() -> Vec<(PathBuf, String)> {
        let store_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../cipher-store-cf");
        let mut paths = vec![
            store_root.join("src/endpoints/blob.ts"),
            store_root.join("src/lib/storage-grant.ts"),
        ];
        let mut migrations = fs::read_dir(store_root.join("migrations"))
            .expect("read shipping cipher-store migrations")
            .map(|entry| entry.expect("migration directory entry").path())
            .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("sql"))
            .collect::<Vec<_>>();
        migrations.sort();
        paths.extend(migrations);
        paths
            .into_iter()
            .map(|path| {
                let source = fs::read_to_string(&path)
                    .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
                (path, source)
            })
            .collect()
    }

    fn assert_shipping_store_has_no_account_fields() {
        let artifacts = shipping_store_artifacts();
        let occurrences = artifacts
            .iter()
            .flat_map(|(path, source)| {
                let source = source.to_ascii_lowercase();
                ACCOUNT_FIELDS.into_iter().filter_map(move |field| {
                    source
                        .contains(field)
                        .then(|| format!("{}:{field}", path.display()))
                })
            })
            .collect::<Vec<_>>();
        assert!(
            occurrences.is_empty(),
            "TASK4613_PRIVACY_LEAK shipping cipher-store account fields: {}",
            occurrences.join(","),
        );

        let verifier = artifacts
            .iter()
            .find(|(path, _)| path.ends_with("src/lib/storage-grant.ts"))
            .map(|(_, source)| source)
            .expect("shipping storage grant verifier");
        assert!(verifier.contains("Object.keys(claims).length !== 3"));
        assert!(verifier.contains("![\"aud\", \"exp\", \"jti\"].every"));
        println!(
            "TASK4613_SHIPPING_STORE_SCAN files={} account_fields={} grant_fields=aud,exp,jti",
            artifacts.len(),
            occurrences.len(),
        );
    }

    #[test]
    fn task_4613_metering_never_links_the_buyer_to_five_blobs() {
        assert_shipping_store_has_no_account_fields();
        let buyer = BuyerIdentity {
            account_id: "acct_4613_alice",
            stripe_customer_id: "cus_4613_private",
            email: "alice-4613@example.test",
            public_handle: "@alice4613",
            device_name: "Alice private laptop",
        };
        let invoice = CryptoTopUpInvoice::new(
            "cpay_4613aaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            CryptoCoin::Monero,
            TopUpPack::TinyTest,
            MONERO_AMOUNT_ATOMIC,
            XMR_CONFIRMATIONS,
        );
        let payment = CryptoInvoicePayment::new(
            invoice.invoice_id(),
            CryptoCoin::Monero,
            MONERO_AMOUNT_ATOMIC,
            XMR_CONFIRMATIONS,
        );
        let mut account = MonthlyAllowanceAccount::new(ACCOUNT_RESET);
        let mut keyserver = CryptoTopUpInvoiceProcessor::new();
        let credited = keyserver
            .apply_at(&mut account, SETTLED_AT, &invoice, &payment)
            .expect("the confirmed credit sale must redeem one anonymous top-up");
        assert_eq!(account.redeemed_top_up_count(), 1);
        assert_eq!(keyserver.payment_records().len(), 1);
        println!(
            "TASK4613_PURCHASE accounts=1 credit_sales={} top_up_credits={} cap_bytes={}",
            keyserver.payment_records().len(),
            account.redeemed_top_up_count(),
            credited.cap_bytes(),
        );

        let mut store_records = Vec::new();
        let mut blob_ids = Vec::new();
        let mut spent_bytes = 0_u64;
        for index in 0..BLOB_COUNT {
            let blob_id = format!("{:032x}", 0xb100_u128 + index as u128);
            let byte_count = 1_024_u64 * (index as u64 + 1);
            account
                .meter_mut()
                .record_send_at(
                    SETTLED_AT + index as i64 + 1,
                    MeteredSendPath::Attachments,
                    byte_count,
                    format!("local-upload-{index}"),
                )
                .expect("meter uploaded bytes");
            spent_bytes += byte_count;
            store_records.push(json!({
                "blob_id": blob_id,
                "size_bytes": byte_count,
                "upload_grant": grant_claims(index),
            }));
            blob_ids.push(blob_id);
        }
        let after_uploads = account
            .snapshot_at(SETTLED_AT + BLOB_COUNT as i64 + 1)
            .expect("read locally spent bytes");
        assert_eq!(after_uploads.spent_bytes(), spent_bytes);
        assert_eq!(store_records.len(), BLOB_COUNT);
        println!(
            "TASK4613_UPLOADS blob_count={} spent_bytes={} remaining_bytes={}",
            store_records.len(),
            after_uploads.spent_bytes(),
            after_uploads.remaining_bytes(),
        );

        let grant_fields = visible_grant_fields(&store_records);
        assert_eq!(grant_fields, ["aud", "exp", "jti"]);
        for record in &store_records {
            let claims = record["upload_grant"].as_object().expect("grant claims");
            assert_eq!(claims.len(), 3);
            assert_eq!(claims["aud"], GRANT_AUDIENCE);
        }
        println!(
            "TASK4613_GRANT_FIELDS visible={} field_count={}",
            grant_fields.join(","),
            grant_fields.len(),
        );

        inject_requested_leak(&mut store_records, &buyer);
        let counts = audit_store(&store_records, &buyer);
        println!(
            "TASK4613_STORE_RECORDS blobs={} account_ids={} stripe_customer_ids={} emails={} public_handles={} device_names={}",
            store_records.len(),
            counts.account_ids,
            counts.stripe_customer_ids,
            counts.emails,
            counts.public_handles,
            counts.device_names,
        );
        if counts.total() != 0 {
            eprintln!(
                "TASK4613_PRIVACY_LEAK account id beside {} blobs account_fields_beside_blob={}",
                counts.account_ids,
                counts.total(),
            );
        }
        assert_eq!(
            counts.total(),
            0,
            "TASK4613_PRIVACY_LEAK test exits nonzero when an account field appears beside a blob",
        );

        let sale = &keyserver.payment_records()[0];
        let keyserver_print = format!(
            "TASK4613_KEYSERVER credit_sale invoice={} coin={} amount_atomic={} blob_ids=0",
            sale.invoice_id(),
            sale.coin(),
            sale.amount_atomic(),
        );
        assert!(blob_ids
            .iter()
            .all(|blob_id| !keyserver_print.contains(blob_id)));
        println!("{keyserver_print}");

        let store_print = format!(
            "TASK4613_STORE blob_ids={} buyer_fields=0",
            blob_ids.join(","),
        );
        assert!(blob_ids.iter().all(|blob_id| store_print.contains(blob_id)));
        assert!(buyer
            .values()
            .iter()
            .all(|value| !store_print.contains(value)));
        println!("{store_print}");
        println!(
            "TASK4613_PRIVACY account_fields_beside_blob={} store_blob_ids={} keyserver_blob_ids=0",
            counts.total(),
            blob_ids.len(),
        );
    }
}
