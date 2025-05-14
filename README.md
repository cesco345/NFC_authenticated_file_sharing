# NFC File Sharing System for Raspberry Pi

This guide provides comprehensive instructions for setting up an NFC-authenticated file sharing system on a Raspberry Pi using an ACR122U NFC reader. The system enables secure file sharing between mobile devices and your Raspberry Pi, with access controlled by an NFC card.

## Table of Contents

1. [System Overview](#system-overview)
2. [Hardware Requirements](#hardware-requirements)
3. [Initial Setup](#initial-setup)
4. [ACR122U Reader Configuration](#acr122u-reader-configuration)
5. [Python NFC Detector Script](#python-nfc-detector-script)
6. [Rust File Sharing System](#rust-file-sharing-system)
7. [Mobile Device Connection](#mobile-device-connection)
8. [Running the System at Startup](#running-the-system-at-startup)
9. [Troubleshooting](#troubleshooting)
10. [Advanced Features](#advanced-features)

## System Overview

This NFC file sharing system uses a two-component architecture:

1. **Python NFC Detector Script**: A lightweight Python script that interfaces with the ACR122U reader to detect NFC cards
2. **Rust File Sharing System**: A Rust program that controls file sharing permissions based on NFC authentication

When an authorized NFC card is detected, the system enables both Samba (SMB) and SFTP access to a shared folder for a configurable period (default 10 minutes). After the time expires or when the program exits, all access is automatically revoked.

## Hardware Requirements

- Raspberry Pi (any model with USB ports)
- ACR122U NFC Reader
- MicroSD card with Raspberry Pi OS installed
- NFC cards/tags (MIFARE Classic 1K recommended)
- Power supply for the Raspberry Pi
- Network connection (Wi-Fi or Ethernet)

## Initial Setup

1. Start with a fresh installation of Raspberry Pi OS (formerly Raspbian).

2. Update your system:
   ```bash
   sudo apt update && sudo apt upgrade -y
   ```

3. Install essential tools:
   ```bash
   sudo apt install -y git python3-pip samba samba-common-bin
   ```

4. Install Rust:
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   source $HOME/.cargo/env
   ```

## ACR122U Reader Configuration

1. Install the required libraries:
   ```bash
   sudo apt install -y pcscd pcsc-tools libacsccid1 libpcsclite-dev python3-pyscard
   sudo apt install -y libnfc-bin libnfc-dev libnfc5
   ```

2. Create the NFC configuration file:
   ```bash
   sudo mkdir -p /etc/nfc
   sudo nano /etc/nfc/libnfc.conf
   ```

3. Add these configuration settings:
   ```
   # Allow device auto-detection
   allow_autoscan = false
   allow_intrusive_scan = false
   log_level = 3

   # Force the specific driver for ACR122U
   device.driver = "acr122_pcsc"
   device.name = "ACS ACR122U PICC Interface"
   ```

4. Create a blacklist file to prevent conflicts:
   ```bash
   sudo nano /etc/modprobe.d/blacklist-nfc.conf
   ```

5. Add these lines:
   ```
   blacklist pn533
   blacklist pn533_usb
   blacklist nfc
   ```

6. Create udev rules for the ACR122U:
   ```bash
   sudo nano /etc/udev/rules.d/99-acr122.rules
   ```

7. Add this content:
   ```
   # ACR122U NFC Reader
   SUBSYSTEM=="usb", ATTRS{idVendor}=="072f", ATTRS{idProduct}=="2200", GROUP="plugdev", MODE="0666"
   ```

8. Apply the changes:
   ```bash
   sudo udevadm control --reload-rules
   sudo udevadm trigger
   sudo systemctl restart pcscd
   ```

9. Test the reader:
   ```bash
   sudo pcsc_scan
   ```
   Place an NFC card on the reader - the output should show "Card inserted" and display the card's ATR.

## Python NFC Detector Script

1. Create the NFC detector script:
   ```bash
   sudo nano ~/nfc_detector.py
   ```

2. Add this content:
   ```python
   #!/usr/bin/env python3
   from smartcard.System import readers
   from smartcard.util import toHexString
   from smartcard.Exceptions import NoCardException, CardConnectionException

   try:
       r = readers()
       if len(r) == 0:
           print("NO_READERS")
           exit(1)
       
       conn = r[0].createConnection()
       try:
           conn.connect()
           GET_UID = [0xFF, 0xCA, 0x00, 0x00, 0x00]
           response, sw1, sw2 = conn.transmit(GET_UID)
           if sw1 == 0x90 and sw2 == 0x00:
               print(toHexString(response))
           else:
               print("ERROR")
       except NoCardException:
           print("NO_CARD")
       except CardConnectionException:
           print("CONNECT_ERROR")
       finally:
           try:
               conn.disconnect()
           except:
               pass
   except Exception as e:
       print(f"EXCEPTION: {e}")
       exit(1)
   ```

3. Make the script executable:
   ```bash
   chmod +x ~/nfc_detector.py
   ```

4. Test the script:
   ```bash
   sudo python3 ~/nfc_detector.py
   ```
   Place an NFC card on the reader. The script should output the card's UID (e.g., "79 DE 3F 02").


## Mobile Device Connection

### Android Devices with FX File Explorer

1. Install FX File Explorer from the Google Play Store.

2. For SFTP connection (recommended):
   - Tap the "+" button
   - Select "Network Storage"
   - Choose "SSH FTP Server"
   - Enter details:
     - Name: Raspberry Pi
     - Host: [Your Pi's IP address]
     - Username: fileuser
     - Password: [The password you set for fileuser]
     - Initial folder: /home/pi/file_share
   - Tap "Test" then "Save"

3. For SMB connection:
   - Tap the "+" button
   - Select "Network Storage"
   - Choose "Windows Share (SMB)"
   - Enter details:
     - Name: Raspberry Pi SMB
     - Server: [Your Pi's IP address]
     - Share: FileShare
     - Username: pi
     - Password: [Your Samba password]
   - Tap "Test" then "Save"

### iOS Devices

1. Use the Files app:
   - Tap "Browse"
   - Tap "Connect to Server"
   - Enter "smb://[Your Pi's IP address]/FileShare"
   - Enter username (pi) and password

## Running the System at Startup

To have the NFC file sharing system start automatically at boot:

1. Create a systemd service file:
   ```bash
   sudo nano /etc/systemd/system/nfc-fileshare.service
   ```

2. Add this content:
   ```
   [Unit]
   Description=NFC File Sharing Service
   After=network.target

   [Service]
   ExecStart=/home/pi/rust-nfc-fileshare/target/release/nfc-fileshare
   WorkingDirectory=/home/pi
   StandardOutput=inherit
   StandardError=inherit
   Restart=always
   User=root

   [Install]
   WantedBy=multi-user.target
   ```

3. Enable and start the service:
   ```bash
   sudo systemctl enable nfc-fileshare
   sudo systemctl start nfc-fileshare
   ```

4. Check the status:
   ```bash
   sudo systemctl status nfc-fileshare
   ```

## Troubleshooting

### ACR122U Not Detected

1. Check if the reader is physically connected:
   ```bash
   lsusb
   ```
   Look for "Advanced Card Systems, Ltd ACR122U"

2. Try restarting the PC/SC daemon:
   ```bash
   sudo systemctl restart pcscd
   ```

3. Make sure your blacklist file is correctly configured and reboot:
   ```bash
   sudo reboot
   ```

### File Sharing Not Working

1. Check if the Python script correctly detects cards:
   ```bash
   sudo python3 ~/nfc_detector.py
   ```

2. Verify Samba is running:
   ```bash
   sudo systemctl status smbd
   ```

3. Check the authentication log:
   ```bash
   cat ~/nfc_auth.log
   ```

4. Verify file permissions:
   ```bash
   ls -la ~/file_share
   ```

### Mobile Device Can't Connect

1. Ensure the Pi and mobile device are on the same network.

2. Check for firewall issues:
   ```bash
   sudo ufw status
   ```

3. If active, allow Samba and SSH:
   ```bash
   sudo ufw allow samba
   sudo ufw allow ssh
   ```

## Advanced Features

### Customizing Timeout Duration

To change the timeout period (default: 10 minutes), modify the `TIMEOUT_MINUTES` constant in the Rust code.

### Adding Multiple Authorized Cards

To add more cards, place each card on the reader and run:
```bash
sudo python3 ~/nfc_detector.py
```

Note the UID output and add it to the `AUTHORIZED_UIDS` array in the Rust code:
```rust
const AUTHORIZED_UIDS: [&str; 2] = ["79 DE 3F 02", "AA BB CC DD"];
```

### Using LED Feedback

The ACR122U has built-in LEDs and a buzzer that can be controlled with APDU commands. You can add visual feedback by implementing these commands in your system.

## Complete Rust Implementation

```rust
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
```
