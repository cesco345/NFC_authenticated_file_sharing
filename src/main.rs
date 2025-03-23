use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::{Duration, SystemTime};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use chrono::{NaiveDateTime, Local};

// Constants
const SHARE_PATH: &str = "/home/pi/file_share";
const AUTH_LOG: &str = "/home/pi/nfc_auth.log";
const AUTH_STATE: &str = "/home/pi/auth_state";
const TIMEOUT_MINUTES: i64 = 10;
const NFC_SCRIPT: &str = "/home/pi/nfc_detector.py";

// List of authorized UIDs - replace with your actual card UIDs
const AUTHORIZED_UIDS: [&str; 1] = ["79 DE 3F 02"];

fn main() -> io::Result<()> {
    println!("\n===== Rust NFC File Sharing System =====");
    println!("Place your NFC card on the reader to enable file sharing");
    println!("Press Ctrl+C to exit");
    println!("===================================\n");

    // Ensure the script exists
    if !Path::new(NFC_SCRIPT).exists() {
        println!("Error: NFC detector script not found at {}", NFC_SCRIPT);
        println!("Please create the script first");
        return Ok(());
    }

    // Ensure the share directory exists and system is setup
    setup_system()?;

    // Ensure sharing is initially disabled
    disable_file_sharing()?;

    // Setup signal handling for clean shutdown
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        println!("\nShutting down NFC File Sharing service...");
        let _ = disable_file_sharing();
        let _ = cleanup();  // Added cleanup here
        r.store(false, Ordering::SeqCst);
    }).expect("Error setting Ctrl-C handler");

    let mut last_check = SystemTime::now();
    
    while running.load(Ordering::SeqCst) {
        // Check if current auth is still valid
        if check_auth_state()? {
            let now = SystemTime::now();
            if now.duration_since(last_check).unwrap_or(Duration::from_secs(0)) > Duration::from_secs(30) {
                // Display status every 30 seconds
                if let Ok(expiration_str) = fs::read_to_string(AUTH_STATE) {
                    if let Ok(expiration) = NaiveDateTime::parse_from_str(&expiration_str.trim(), "%Y-%m-%d %H:%M:%S") {
                        let now = Local::now().naive_local();
                        if expiration > now {
                            let duration = expiration.signed_duration_since(now);
                            let mins = duration.num_minutes();
                            let secs = duration.num_seconds() % 60;
                            println!("File sharing active. Time remaining: {:02}:{:02}", mins, secs);
                        }
                    }
                }
                last_check = now;
            }
            thread::sleep(Duration::from_secs(1));
            continue;
        }

        // Reset last_check when not authenticated
        last_check = SystemTime::now();

        // Read NFC card using external Python script
        match read_card_uid() {
            Some(uid) => {
                println!("\nCard detected: {}", uid);
                
                if AUTHORIZED_UIDS.contains(&uid.as_str()) {
                    log_event(&format!("Authorized card: {}", uid))?;
                    enable_file_sharing()?;
                } else {
                    log_event(&format!("Unauthorized card: {}", uid))?;
                    println!("❗ Unauthorized card");
                    disable_file_sharing()?;
                }
                
                // Wait a moment before scanning again
                thread::sleep(Duration::from_secs(2));
            },
            None => {
                // Small delay to prevent CPU usage spikes
                thread::sleep(Duration::from_millis(500));
            }
        }
    }

    // Make sure cleanup runs at the end no matter what
    cleanup()?;
    
    Ok(())
}

fn read_card_uid() -> Option<String> {
    // Execute the Python script
    match Command::new("sudo")
        .arg("python3")
        .arg(NFC_SCRIPT)
        .output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            
            if !stdout.is_empty() && stdout != "NO_CARD" && 
               stdout != "ERROR" && stdout != "NO_READERS" && 
               stdout != "CONNECT_ERROR" && !stdout.starts_with("EXCEPTION:") {
                return Some(stdout);
            }
            None
        },
        Err(_) => None
    }
}

fn setup_system() -> io::Result<()> {
    println!("Setting up system requirements...");
    
    // Ensure the share directory exists
    if !Path::new(SHARE_PATH).exists() {
        fs::create_dir_all(SHARE_PATH)?;
    }
    
    // Check if Samba is already running
    let status = Command::new("systemctl")
        .args(["status", "smbd"])
        .output()?;
    
    if !status.status.success() || !String::from_utf8_lossy(&status.stdout).contains("active (running)") {
        println!("Samba is not running. Setting up Samba...");
        
        // Install Samba
        let _ = Command::new("sudo")
            .args(["apt", "install", "-y", "samba", "samba-common-bin"])
            .status()?;
        
        // Configure Samba
        let smb_config = format!(r#"
[FileShare]
   path = {}
   browseable = yes
   writable = yes
   guest ok = no
   create mask = 0777
   directory mask = 0777
"#, SHARE_PATH);
        
        let mut file = File::create("/tmp/smb.conf.addition")?;
        file.write_all(smb_config.as_bytes())?;
        
        let _ = Command::new("sudo")
            .args(["bash", "-c", "cat /tmp/smb.conf.addition >> /etc/samba/smb.conf"])
            .status()?;
        
        // Note: Setting Samba password requires interactive input
        println!("Please set a Samba password for the pi user if you haven't already:");
        println!("Run: sudo smbpasswd -a pi");
        
        // Start Samba service
        let _ = Command::new("sudo")
            .args(["systemctl", "restart", "smbd"])
            .status()?;
        
        println!("Samba setup complete!");
    } else {
        println!("Samba is already running.");
    }
    
    // Check if fileuser exists
    let fileuser_check = Command::new("id")
        .args(["-u", "fileuser"])
        .output()?;
    
    if !fileuser_check.status.success() {
        println!("Creating dedicated SFTP user...");
        
        // Create fileuser
        let _ = Command::new("sudo")
            .args(["adduser", "fileuser", "--gecos", "\"\"", "--disabled-password"])
            .status()?;
        
        println!("SFTP user created. You'll need to set a password manually:");
        println!("Run: sudo passwd fileuser");
    } else {
        println!("SFTP user already exists.");
    }
    
    // Ensure fileuser is initially locked
    let _ = Command::new("sudo")
        .args(["usermod", "-L", "fileuser"])
        .status()?;
    
    Ok(())
}

fn enable_file_sharing() -> io::Result<()> {
    // Create state file with expiration time
    let now = Local::now().naive_local();
    let expiration = now + chrono::Duration::minutes(TIMEOUT_MINUTES);
    
    // Format without timezone for simpler parsing
    let expiration_str = expiration.format("%Y-%m-%d %H:%M:%S").to_string();
    
    fs::write(AUTH_STATE, expiration_str)?;
    
    // Make the share directory accessible for Samba
    let _ = Command::new("sudo")
        .args(["chmod", "-R", "777", SHARE_PATH])
        .status()?;
    
    // Set up permissions for SFTP access
    let _ = Command::new("sudo")
        .args(["chown", "-R", "pi:fileuser", SHARE_PATH])
        .status()?;
    
    let _ = Command::new("sudo")
        .args(["chmod", "-R", "770", SHARE_PATH])
        .status()?;
    
    // Enable the fileuser account
    let _ = Command::new("sudo")
        .args(["usermod", "-U", "fileuser"])
        .status()?;
    
    // Make sure Samba service is running
    let _ = Command::new("sudo")
        .args(["systemctl", "restart", "smbd"])
        .status()?;
    
    log_event(&format!("File sharing enabled for {} minutes", TIMEOUT_MINUTES))?;
    
    // Get IP address
    let ip_output = Command::new("hostname")
        .arg("-I")
        .output()?;
    let ip = String::from_utf8_lossy(&ip_output.stdout)
        .trim()
        .split_whitespace()
        .next()
        .unwrap_or("localhost")
        .to_string();
    
    println!("\n✅ File sharing ENABLED for {} minutes", TIMEOUT_MINUTES);
    println!("📁 Shared folder: {}", SHARE_PATH);
    println!("💻 Connect via SMB: smb://{}/FileShare", ip);
    println!("💻 Connect via SFTP: sftp://{}/home/pi/file_share", ip);
    println!("👤 SMB Username: pi");
    println!("👤 SFTP Username: fileuser");
    println!("⏱️  Timeout: {}", expiration.format("%H:%M:%S"));
    
    Ok(())
}

fn disable_file_sharing() -> io::Result<()> {
    println!("Disabling file sharing...");
    
    if Path::new(AUTH_STATE).exists() {
        fs::remove_file(AUTH_STATE)?;
    }
    
    // Restrict permissions for both Samba and SFTP
    let _ = Command::new("sudo")
        .args(["chmod", "-R", "700", SHARE_PATH])
        .status()?;
    
    let _ = Command::new("sudo")
        .args(["chown", "-R", "pi:pi", SHARE_PATH])
        .status()?;
    
    // Disable the fileuser account with stronger command
    let _ = Command::new("sudo")
        .args(["usermod", "-L", "fileuser"])
        .status()?;
    
    // Force a restart of Samba to drop connections
    let _ = Command::new("sudo")
        .args(["systemctl", "restart", "smbd"])
        .status()?;
    
    log_event("File sharing disabled");
    println!("\n❌ File sharing DISABLED");
    
    Ok(())
}

fn cleanup() -> io::Result<()> {
    println!("Running complete cleanup...");
    
    // Restrict permissions extremely tightly
    let _ = Command::new("sudo")
        .args(["chmod", "-R", "700", SHARE_PATH])
        .status()?;
    
    let _ = Command::new("sudo")
        .args(["chown", "-R", "pi:pi", SHARE_PATH])
        .status()?;
    
    // Ensure fileuser is locked
    let _ = Command::new("sudo")
        .args(["usermod", "-L", "fileuser"])
        .status()?;
    
    // Stop Samba service to force disconnect all clients
    let _ = Command::new("sudo")
        .args(["systemctl", "restart", "smbd"])
        .status()?;
    
    // Remove the auth state file
    if Path::new(AUTH_STATE).exists() {
        fs::remove_file(AUTH_STATE)?;
    }
    
    log_event("System completely cleaned up on exit")?;
    println!("Cleanup complete, all access should be disabled");
    
    Ok(())
}

fn check_auth_state() -> io::Result<bool> {
    if !Path::new(AUTH_STATE).exists() {
        return Ok(false);
    }
    
    let expiration_str = fs::read_to_string(AUTH_STATE)?;
    
    match NaiveDateTime::parse_from_str(&expiration_str.trim(), "%Y-%m-%d %H:%M:%S") {
        Ok(expiration) => {
            let now = Local::now().naive_local();
            if now > expiration {
                disable_file_sharing()?;
                Ok(false)
            } else {
                Ok(true)
            }
        },
        Err(_) => {
            // Failed to parse expiration, so disable sharing
            disable_file_sharing()?;
            Ok(false)
        }
    }
}

fn log_event(message: &str) -> io::Result<()> {
    let now = Local::now();
    let timestamp = now.format("%Y-%m-%d %H:%M:%S");
    
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(AUTH_LOG)?;
    
    writeln!(file, "[{}] {}", timestamp, message)?;
    Ok(())
}
