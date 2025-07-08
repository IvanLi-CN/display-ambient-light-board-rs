fn main() {
    // Load .env file for WiFi configuration
    load_env_config();

    linker_be_nice();
    // make sure linkall.x is the last linker script (otherwise might cause problems with flip-link)
    println!("cargo:rustc-link-arg=-Tlinkall.x");
}

/// Load environment configuration from .env file
/// Environment variables take priority over .env file values
fn load_env_config() {
    use std::env;
    use std::path::Path;

    // Tell cargo to rerun this build script if .env file changes
    println!("cargo:rerun-if-changed=.env");

    // Tell cargo to rerun if environment variables change
    println!("cargo:rerun-if-env-changed=WIFI_SSID");
    println!("cargo:rerun-if-env-changed=WIFI_PASSWORD");

    // Try to load .env file if it exists
    if Path::new(".env").exists() {
        match dotenvy::dotenv() {
            Ok(_) => println!("cargo:warning=Loaded .env file"),
            Err(e) => println!("cargo:warning=Failed to load .env file: {}", e),
        }
    }

    // Get WiFi credentials with fallbacks
    // Note: We need to handle the case where env vars are set to empty strings
    let wifi_ssid = env::var("WIFI_SSID")
        .unwrap_or_else(|_| String::new())
        .trim()
        .to_string();
    let wifi_password = env::var("WIFI_PASSWORD")
        .unwrap_or_else(|_| String::new())
        .trim()
        .to_string();

    // Set environment variables for the compilation
    println!("cargo:rustc-env=WIFI_SSID={}", wifi_ssid);
    println!("cargo:rustc-env=WIFI_PASSWORD={}", wifi_password);

    // Print status
    if wifi_ssid.is_empty() {
        println!("cargo:warning=WIFI_SSID is empty - WiFi will not be configured");
    } else {
        println!("cargo:warning=WIFI_SSID configured: {}", wifi_ssid);
    }

    if wifi_password.is_empty() {
        println!("cargo:warning=WIFI_PASSWORD is empty - WiFi will not be configured");
    } else {
        println!("cargo:warning=WIFI_PASSWORD configured (length: {})", wifi_password.len());
    }
}

fn linker_be_nice() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        let kind = &args[1];
        let what = &args[2];

        match kind.as_str() {
            "undefined-symbol" => match what.as_str() {
                "_defmt_timestamp" => {
                    eprintln!();
                    eprintln!("💡 `defmt` not found - make sure `defmt.x` is added as a linker script and you have included `use defmt_rtt as _;`");
                    eprintln!();
                }
                "_stack_start" => {
                    eprintln!();
                    eprintln!("💡 Is the linker script `linkall.x` missing?");
                    eprintln!();
                }
                "esp_wifi_preempt_enable"
                | "esp_wifi_preempt_yield_task"
                | "esp_wifi_preempt_task_create" => {
                    eprintln!();
                    eprintln!("💡 `esp-wifi` has no scheduler enabled. Make sure you have the `builtin-scheduler` feature enabled, or that you provide an external scheduler.");
                    eprintln!();
                }
                "embedded_test_linker_file_not_added_to_rustflags" => {
                    eprintln!();
                    eprintln!("💡 `embedded-test` not found - make sure `embedded-test.x` is added as a linker script for tests");
                    eprintln!();
                }
                _ => (),
            },
            // we don't have anything helpful for "missing-lib" yet
            _ => {
                std::process::exit(1);
            }
        }

        std::process::exit(0);
    }

    println!(
        "cargo:rustc-link-arg=--error-handling-script={}",
        std::env::current_exe().unwrap().display()
    );
}
