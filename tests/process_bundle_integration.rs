use arweave_ans_1040_indexer::process_bundle;
use fastuuid::Generator;
use std::fs;
use std::io::Cursor;
use std::io::Write;
use tokio::sync::mpsc;

async fn run_process_bundle_test(tx_id: &str, expected_output_path: &str) {
    let generator = Generator::new();
    let uuid = generator.hex128_as_string().unwrap();
    let actual_output_path = format!("tests/samples/actual_output_{}", uuid);

    let response_bytes = reqwest::get(format!("https://arweave.net/{}", tx_id))
        .await
        .expect("Failed to fetch transaction data")
        .bytes()
        .await
        .expect("Failed to read response bytes");

    let mut cursor = Cursor::new(response_bytes);
    let (tx, mut rx) = mpsc::channel(10);

    // Spawn a task to collect items
    let read_handle = tokio::spawn(async move {
        let mut items = Vec::new();
        while let Some(item) = rx.recv().await {
            items.push(item);
        }
        items
    });

    process_bundle(&mut cursor, tx, tx_id).await.unwrap();

    let items = read_handle.await.unwrap();

    let mut file = fs::File::create(&actual_output_path).expect("Failed to create output file");

    for item in &items {
        file.write_all(
            serde_json::to_string_pretty(&item)
                .expect("Failed to serialize item")
                .as_bytes(),
        )
        .expect("Failed to write to file");
        file.write_all(b"\n").expect("Failed to write newline");
    }

    let cleanup = Cleanup {
        file_path: actual_output_path.clone(),
    };

    let expected_output =
        fs::read_to_string(expected_output_path).expect("Failed to read expected output");

    let actual_output =
        fs::read_to_string(&actual_output_path).expect("Failed to read actual output");

    assert_eq!(actual_output, expected_output, "The outputs do not match");

    cleanup.clean();
}

struct Cleanup {
    file_path: String,
}

impl Cleanup {
    fn clean(&self) {
        let _ = fs::remove_file(&self.file_path);
    }
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        self.clean();
    }
}

#[tokio::test]
async fn test_process_bundle_integration_small() {
    let tx_id = "K0JskpURZ-zZ7m01txR7hArvsBDDi08S6-6YIVQoc_Y";
    let expected_output_path = "tests/samples/small";

    run_process_bundle_test(tx_id, expected_output_path).await;
}

#[tokio::test]
async fn test_process_bundle_integration_big_nested() {
    let tx_id = "H95gGHbh3dbpCCLAk36sNHCOCgsZ1hy8IG9IEXDNl3o";
    let expected_output_path = "tests/samples/big";

    run_process_bundle_test(tx_id, expected_output_path).await;
}
