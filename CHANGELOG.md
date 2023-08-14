diff --git a/Cargo.toml b/Cargo.toml
index 8b6fe08..eba096b 100644
--- a/Cargo.toml
+++ b/Cargo.toml
@@ -15,7 +15,7 @@ version = "0.2.1"
[dependencies]
fuzzy-matcher = "0.3.7"
serde = { version = "1", optional = true, features = ["derive"] }
-time = { version = "0.3.11", optional = true, features = ["local-offset"] }
+time = { version = "0.3.25", optional = true, features = ["local-offset"] }
tui = { package = "ratatui", version = "0.22.0", features = ["all-widgets"] }
unicode-segmentation = "1.10"
unicode-width = "0.1"
@@ -24,7 +24,7 @@ tracing = "0.1.37"
rayon = "1.7.0"

[dev-dependencies]
-anyhow = "1.0.71"
+anyhow = "1.0.72"
criterion = { version = "0.5", features = ["html_reports"] }
fakeit = "1.1"
rand = "0.8"
