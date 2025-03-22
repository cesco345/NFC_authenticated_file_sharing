# NFC File Sharing System for Raspberry Pi

This guide provides comprehensive instructions for setting up an NFC-authenticated file sharing system on a Raspberry Pi using an ACR122U NFC reader. The system allows you to securely share files between mobile devices and your Raspberry Pi by authenticating with an NFC card/tag.

## Table of Contents

1. [Hardware Requirements](#hardware-requirements)
2. [Initial Setup](#initial-setup)
3. [Installing ACR122U Dependencies](#installing-acr122u-dependencies)
4. [Troubleshooting ACR122U Issues](#troubleshooting-acr122u-issues)
5. [Setting Up the NFC File Sharing System](#setting-up-the-nfc-file-sharing-system)
6. [Connecting from Mobile Devices](#connecting-from-mobile-devices)
   - [Using FX File Explorer](#using-fx-file-explorer)
   - [Using SSH/SFTP Server](#using-sshsftp-server)
7. [Running the System at Startup](#running-the-system-at-startup)
8. [Rust Implementation (Optional)](#rust-implementation-optional)
9. [Advanced NFC Card Features](#advanced-nfc-card-features)
10. [Troubleshooting & FAQs](#troubleshooting--faqs)

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

## Installing ACR122U Dependencies

1. Install the required libraries for the ACR122U NFC reader:
   ```bash
   sudo apt install -y pcscd pcsc-tools libacsccid1 libpcsclite-dev python3-pyscard
   ```

2. Install libnfc libraries:
   ```bash
   sudo apt install -y libnfc-bin libnfc-dev libnfc5
   ```

3. Create the necessary configuration files:
   ```bash
   sudo mkdir -p /etc/nfc
   sudo nano /etc/nfc/libnfc.conf
   ```

4. Add the following content to the libnfc.conf file:
   ```
   # Allow device auto-detection
   allow_autoscan = false
   allow_intrusive_scan = false
   log_level = 3

   # Force the specific driver for ACR122U
   device.driver = "acr122_pcsc"
   device.name = "ACS ACR122U PICC Interface"
   ```

5. Create a blacklist file to prevent conflicts with kernel modules:
   ```bash
   sudo nano /etc/modprobe.d/blacklist-nfc.conf
   ```

6. Add the following content:
   ```
   blacklist pn533
   blacklist pn533_usb
   blacklist nfc
   ```

7. Create udev rules for the ACR122U:
   ```bash
   sudo nano /etc/udev/rules.d/99-acr122.rules
   ```

8. Add the following content:
   ```
   # ACR122U NFC Reader
   SUBSYSTEM=="usb", ATTRS{idVendor}=="072f", ATTRS{idProduct}=="2200", GROUP="plugdev", MODE="0666"
   ```

9. Apply the changes:
   ```bash
   sudo udevadm control --reload-rules
   sudo udevadm trigger
   ```

10. Restart the PC/SC daemon:
    ```bash
    sudo systemctl restart pcscd
    ```

## Troubleshooting ACR122U Issues

If you experience issues with the ACR122U reader, try these troubleshooting steps:

1. Check if the reader is detected by the system:
   ```bash
   lsusb
   ```
   You should see a device listed as "Advanced Card Systems, Ltd ACR122U".

2. Test if the PC/SC system detects the reader:
   ```bash
   sudo pcsc_scan
   ```
   This should show your reader. Place a card on the reader to see if it detects it.

3. If pcsc_scan works but nfc-list doesn't, it's likely a configuration issue. Try:
   ```bash
   sudo nano /etc/nfc/libnfc.conf
   ```
   
   And simplify to just:
   ```
   # Configuration for accessing ACR122U using pcscd
   device.connstring = "pcscd"
   ```

4. Restart the PC/SC daemon after any configuration changes:
   ```bash
   sudo systemctl stop pcscd
   sudo killall pcscd 2>/dev/null
   sudo rm -f /var/run/pcscd/pcscd.pid
   sudo systemctl start pcscd
   ```

5. If problems persist, create a simple Python test script:
   ```bash
   nano test_nfc.py
   ```
   
   Add the following code:
   ```python
   #!/usr/bin/env python3
   import time
   import sys
   from smartcard.System import readers
   from smartcard.util import toHexString

   # Wait for the reader to settle
   time.sleep(1)

   # Get available readers
   r = readers()
   print("Available readers:", r)

   if len(r) < 1:
       print("No readers found")
       sys.exit(1)

   # Connect to the first reader
   connection = r[0].createConnection()
   try:
       connection.connect()
       print("Connected to:", r[0])
       
       # Get ATR (Answer To Reset)
       atr = connection.getATR()
       print("Card ATR:", toHexString(atr))
       
       # Try a simple APDU command (Get UID)
       GET_UID = [0xFF, 0xCA, 0x00, 0x00, 0x00]
       response, sw1, sw2 = connection.transmit(GET_UID)
       
       print("Response:", toHexString(response))
       print("Status:", hex(sw1), hex(sw2))
       
       if sw1 == 0x90 and sw2 == 0x00:
           print("Card UID:", toHexString(response))
       else:
           print("Error getting UID")
           
   except Exception as e:
       print("Error:", e)
   finally:
       try:
           connection.disconnect()
       except:
           pass
   ```

6. Make it executable and run it:
   ```bash
   chmod +x test_nfc.py
   ./test_nfc.py
   ```

7. If the Python script works but nfc-list doesn't, you can use the Python approach for the file sharing system.

## Setting Up the NFC File Sharing System

### Enhanced Python Implementation with Both SMB and SFTP Controls

This updated implementation controls both Samba (SMB) and SFTP access with the same NFC card:

1. Create the enhanced NFC file sharing script:
   ```bash
   sudo nano /home/pi/nfc_fileshare.py
   ```

2. Copy and paste the following Python script:
   ```python
   #!/usr/bin/env python3
   import os
   import time
   import sys
   import signal
   import subprocess
   from datetime import datetime, timedelta
   from smartcard.System import readers
   from smartcard.util import toHexString
   from smartcard.Exceptions import NoCardException, CardConnectionException

   # Configuration
   SHARE_PATH = "/home/pi/file_share"
   AUTH_LOG = "/home/pi/nfc_auth.log"
   AUTH_STATE = "/home/pi/auth_state"
   TIMEOUT_MINUTES = 10

   # Authorized UIDs (replace with your actual card UIDs)
   # Format should match the output from the card reader (e.g., "79 DE 3F 02")
   AUTHORIZED_UIDS = [
       "79 DE 3F 02",  # Add your card UID here
       # Add more UIDs as needed
   ]

   def log_event(message):
       """Log events with timestamp"""
       with open(AUTH_LOG, "a") as log_file:
           timestamp = datetime.now().strftime("%Y-%m-%d %H:%M:%S")
           log_file.write(f"[{timestamp}] {message}\n")

   def setup_system():
       """Set up the system requirements"""
       try:
           # Create share directory if it doesn't exist
           os.makedirs(SHARE_PATH, exist_ok=True)
           
           # Check if Samba is installed and running
           result = subprocess.run(["systemctl", "status", "smbd"], 
                                  capture_output=True, text=True)
           if "active (running)" not in result.stdout:
               print("Samba is not running. Setting up Samba...")
               subprocess.run(["sudo", "apt", "install", "-y", "samba", "samba-common-bin"], 
                             check=True)
               
               # Configure Samba
               with open("/tmp/smb.conf.addition", "w") as f:
                   f.write(f"""
   [FileShare]
      path = {SHARE_PATH}
      browseable = yes
      writable = yes
      guest ok = no
      create mask = 0777
      directory mask = 0777
   """)
               
               subprocess.run(["sudo", "bash", "-c", 
                              "cat /tmp/smb.conf.addition >> /etc/samba/smb.conf"])
               
               # Set Samba password for pi user
               print("Setting up Samba user 'pi'...")
               p = subprocess.Popen(["sudo", "smbpasswd", "-a", "pi"],
                                  stdin=subprocess.PIPE)
               p.communicate(input=b"raspberry\nraspberry\n")  # Default password, change as needed
               
               # Start Samba service
               subprocess.run(["sudo", "systemctl", "restart", "smbd"], check=True)
               print("Samba setup complete!")
           else:
               print("Samba is already running.")
           
           # Check if the fileuser exists, create if it doesn't
           result = subprocess.run(["id", "-u", "fileuser"], 
                                  capture_output=True, text=True)
           if "no such user" in result.stderr.lower():
               print("Creating dedicated SFTP user...")
               subprocess.run(["sudo", "adduser", "fileuser", "--gecos", '""', "--disabled-password"], 
                             check=True)
               
               # Set a password for fileuser - change 'password' to your desired password
               p = subprocess.Popen(["sudo", "passwd", "fileuser"],
                                  stdin=subprocess.PIPE)
               p.communicate(input=b"password\npassword\n")  # Change this password!
               
               print("SFTP user created. Remember to use these credentials in FX Explorer:")
               print("Username: fileuser")
               print("Password: password")  # Replace with your actual password
           else:
               print("SFTP user already exists.")
           
           # Ensure the user is initially locked
           subprocess.run(["sudo", "usermod", "-L", "fileuser"], check=True)
           
       except Exception as e:
           print(f"Error setting up system: {e}")
           log_event(f"Error setting up system: {e}")

   def enable_file_sharing():
       """Enable file sharing and set timeout"""
       # Create state file with expiration time
       expiration = datetime.now() + timedelta(minutes=TIMEOUT_MINUTES)
       with open(AUTH_STATE, "w") as state_file:
           state_file.write(expiration.strftime("%Y-%m-%d %H:%M:%S"))
       
       # Make the share directory accessible for Samba
       os.system(f"sudo chmod -R 777 {SHARE_PATH}")
       
       # Set up permissions for SFTP access
       os.system(f"sudo chown -R pi:fileuser {SHARE_PATH}")
       os.system(f"sudo chmod -R 770 {SHARE_PATH}")
       
       # Enable the fileuser account
       os.system("sudo usermod -U fileuser")
       
       # Make sure Samba service is running
       os.system("sudo systemctl restart smbd")
       
       # Get IP address for display
       ip_address = get_ip_address()
       
       log_event("File sharing enabled for 10 minutes")
       print(f"\n✅ File sharing ENABLED for {TIMEOUT_MINUTES} minutes")
       print(f"📁 Shared folder: {SHARE_PATH}")
       print(f"💻 Connect via SMB: smb://{ip_address}/FileShare")
       print(f"💻 Connect via SFTP: sftp://{ip_address}/home/pi/file_share")
       print(f"👤 SMB Username: pi")
       print(f"👤 SFTP Username: fileuser")
       print("⏱️  Timeout: " + expiration.strftime("%H:%M:%S"))

   def disable_file_sharing():
       """Disable file sharing"""
       if os.path.exists(AUTH_STATE):
           os.remove(AUTH_STATE)
       
       # Restrict permissions for both Samba and SFTP
       os.system(f"sudo chmod -R 700 {SHARE_PATH}")
       os.system(f"sudo chown -R pi:pi {SHARE_PATH}")
       
       # Disable the fileuser account
       os.system("sudo usermod -L fileuser")
       
       log_event("File sharing disabled")
       print("\n❌ File sharing DISABLED")

   def check_auth_state():
       """Check if authenticated and not expired"""
       if not os.path.exists(AUTH_STATE):
           return False
       
       with open(AUTH_STATE, "r") as state_file:
           expiration_str = state_file.read().strip()
       
       try:
           expiration = datetime.strptime(expiration_str, "%Y-%m-%d %H:%M:%S")
           if datetime.now() > expiration:
               disable_file_sharing()
               return False
           return True
       except Exception:
           return False

   def get_ip_address():
       """Get the device's IP address"""
       try:
           ip = subprocess.check_output(["hostname", "-I"]).decode().strip().split()[0]
           return ip
       except Exception:
           return "localhost"

   def read_card_uid():
       """Read NFC card UID using PC/SC interface"""
       try:
           # Get all available readers
           r = readers()
           if len(r) == 0:
               return None
           
           # Connect to the first reader
           connection = r[0].createConnection()
           try:
               connection.connect()
               
               # Get UID using APDU command
               GET_UID = [0xFF, 0xCA, 0x00, 0x00, 0x00]
               response, sw1, sw2 = connection.transmit(GET_UID)
               
               if sw1 == 0x90 and sw2 == 0x00:
                   uid = toHexString(response)
                   return uid
               else:
                   return None
                   
           except (NoCardException, CardConnectionException):
               return None
           finally:
               try:
                   connection.disconnect()
               except:
                   pass
       except Exception as e:
           log_event(f"Error reading card: {e}")
           return None

   def control_led_and_buzzer(success=True):
       """Control the ACR122U LED and buzzer based on authentication result"""
       try:
           if success:
               # Green LED on for 1 second with beep (success)
               LED_COMMAND = [0xFF, 0x00, 0x40, 0x0E, 0x04, 0x0A, 0x0A, 0x01, 0x01]
           else:
               # Red LED blink 3 times with beep (failure)
               LED_COMMAND = [0xFF, 0x00, 0x40, 0xD0, 0x04, 0x02, 0x02, 0x03, 0x01]
               
           # Get all available readers
           r = readers()
           if len(r) == 0:
               return
               
           # Connect to the first reader
           connection = r[0].createConnection()
           try:
               connection.connect()
               connection.transmit(LED_COMMAND)
           except Exception:
               pass
           finally:
               try:
                   connection.disconnect()
               except:
                   pass
       except Exception:
           # Silently fail if LED control doesn't work
           pass

   def handle_exit(signal, frame):
       """Handle exit gracefully"""
       print("\nShutting down NFC File Sharing service...")
       disable_file_sharing()
       sys.exit(0)

   def main():
       # Setup signal handlers
       signal.signal(signal.SIGINT, handle_exit)
       signal.signal(signal.SIGTERM, handle_exit)
       
       # Setup the system requirements
       setup_system()
       
       # Initialize - ensure sharing is disabled at start
       disable_file_sharing()
       
       print("\n===== NFC File Sharing System =====")
       print("Place your NFC card on the reader to enable file sharing")
       print("Press Ctrl+C to exit")
       print("===================================\n")
       
       last_check = 0
       
       try:
           while True:
               current_time = time.time()
               
               # Check if current auth is still valid
               if check_auth_state():
                   if current_time - last_check > 30:  # Display status every 30 seconds
                       with open(AUTH_STATE, "r") as state_file:
                           expiration_str = state_file.read().strip()
                       expiration = datetime.strptime(expiration_str, "%Y-%m-%d %H:%M:%S")
                       remaining = expiration - datetime.now()
                       mins, secs = divmod(remaining.seconds, 60)
                       print(f"\rFile sharing active. Time remaining: {mins:02d}:{secs:02d}", end="", flush=True)
                       last_check = current_time
                   time.sleep(1)
                   continue
               
               # Reset last_check when not authenticated
               last_check = 0
               
               # Read NFC card
               uid = read_card_uid()
               
               if uid:
                   print(f"\nCard detected: {uid}")
                   
                   if uid in AUTHORIZED_UIDS:
                       log_event(f"Authorized card: {uid}")
                       control_led_and_buzzer(success=True)
                       enable_file_sharing()
                   else:
                       log_event(f"Unauthorized card: {uid}")
                       control_led_and_buzzer(success=False)
                       print("❗ Unauthorized card")
                       disable_file_sharing()
                   
                   # Wait a moment before scanning again
                   time.sleep(2)
               else:
                   # Small delay to prevent CPU usage spikes
                   time.sleep(0.5)
                   
       except KeyboardInterrupt:
           pass
       finally:
           disable_file_sharing()
           print("\nExiting...")

   if __name__ == "__main__":
       # Check if running as root
       if os.geteuid() != 0:
           print("This script requires root privileges.")
           print("Please run with sudo: sudo python3 nfc_fileshare.py")
           sys.exit(1)
       main()
   ```

3. Make the script executable:
   ```bash
   sudo chmod +x /home/pi/nfc_fileshare.py
   ```

4. Update the script with your NFC card's UID:
   - Run the test script to get your card's UID:
     ```bash
     ./test_nfc.py
     ```
   - Note the UID output (e.g., "79 DE 3F 02")
   - Edit the nfc_fileshare.py script to update the AUTHORIZED_UIDS list.

5. Run the file sharing script:
   ```bash
   sudo python3 /home/pi/nfc_fileshare.py
   ```

6. Test the system by placing your NFC card on the reader. The system should enable file sharing for 10 minutes.

## Connecting from Mobile Devices

### Using FX File Explorer

1. Install "FX File Explorer" from the Google Play Store on your Android device.

2. Set up SFTP connection (recommended):
   - Open FX File Explorer
   - Tap the "+" button (Add new storage)
   - Select "Network Storage"
   - Choose "SSH FTP Server"
   - Enter the following details:
     - Name: Raspberry Pi SFTP
     - Host: [Your Pi's IP address] (shown in the terminal)
     - Username: fileuser
     - Password: [The password you set for fileuser]
     - Initial folder: /home/pi/file_share
   - Tap "Test" to verify the connection
   - Tap "Save" to add the storage

3. Alternative: Set up SMB connection:
   - Open FX File Explorer
   - Tap the "+" button
   - Select "Network Storage"
   - Choose "Windows Share (SMB)"
   - Enter the following details:
     - Name: Raspberry Pi SMB
     - Server: [Your Pi's IP address]
     - Share: FileShare
     - Username: pi
     - Password: [Your Samba password]
   - Tap "Test" to verify the connection
   - Tap "Save" to add the storage

4. Transfer files:
   - Navigate to the files you want to transfer
   - Select the files using the selection tool
   - Tap "Copy" or "Move"
   - Navigate to your Raspberry Pi storage
   - Tap "Paste" to transfer the files

### Using SSH/SFTP Server

For iOS devices or other SFTP clients:

1. Set up SFTP connection:
   - Open your SFTP app (Files app on iOS, FileZilla on desktop, etc.)
   - Add a new connection with these details:
     - Protocol: SFTP
     - Host/Server: [Your Pi's IP address]
     - Port: 22
     - Username: fileuser
     - Password: [Password you set for fileuser]
     - Initial directory: /home/pi/file_share

2. Connect and transfer files.

## Running the System at Startup

To have the NFC file sharing system start automatically at boot:

1. Create a systemd service file:
   ```bash
   sudo nano /etc/systemd/system/nfc-fileshare.service
   ```

2. Add the following content:
   ```
   [Unit]
   Description=NFC File Sharing Service
   After=network.target

   [Service]
   ExecStart=/usr/bin/python3 /home/pi/nfc_fileshare.py
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

## Rust Implementation (Optional)

For better performance, you can use the enhanced Rust implementation:

1. Install Rust:
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   source $HOME/.cargo/env
   ```

2. Create a new Rust project:
   ```bash
   mkdir -p ~/rust-nfc-fileshare
   cd ~/rust-nfc-fileshare
   ```

3. Configure the project:
   ```bash
   cargo init
   ```

4. Edit the Cargo.toml file:
   ```bash
   nano Cargo.toml
   ```
   
   Add:
   ```toml
   [package]
   name = "nfc-fileshare"
   version = "0.1.0"
   edition = "2021"

   [dependencies]
   chrono = "0.4"
   ctrlc = "3.2"
   ```

5. Edit the main.rs file:
   ```bash
   nano src/main.rs
   ```
   
   Add the following Rust code:
   ```rust
   use std::fs::{self, File, OpenOptions};
   use std::io::{self, Write};
   use std::path::Path;
   use std::process::Command;
   use std::thread;
   use std::time::{Duration, SystemTime};
   use std::sync::atomic::{AtomicBool, Ordering};
   use std::sync::Arc;
   use chrono::{DateTime, Local, TimeDelta};

   // Constants
   const SHARE_PATH: &str = "/home/pi/file_share";
   const AUTH_LOG: &str = "/home/pi/nfc_auth.log";
   const AUTH_STATE: &str = "/home/pi/auth_state";
   const TIMEOUT_MINUTES: i64 = 10;

   // List of authorized UIDs - replace with your actual card UIDs
   const AUTHORIZED_UIDS: [&str; 1] = ["79 DE 3F 02"];

   fn main() -> io::Result<()> {
       println!("\n===== Rust NFC File Sharing System =====");
       println!("Place your NFC card on the reader to enable file sharing");
       println!("Press Ctrl+C to exit");
       println!("===================================\n");

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
                       if let Ok(expiration) = DateTime::parse_from_str(&expiration_str.trim(), "%Y-%m-%d %H:%M:%S %z") {
                           let now = Local::now();
                           if expiration > now {
                               let remaining = expiration.signed_duration_since(now);
                               let mins = remaining.num_minutes();
                               let secs = remaining.num_seconds() % 60;
                               print!("\rFile sharing active. Time remaining: {:02}:{:02}", mins, secs);
                               io::stdout().flush()?;
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

           // Read NFC card UID
           if let Some(uid) = read_card_uid() {
               println!("\nCard detected: {}", uid);

               if AUTHORIZED_UIDS.contains(&uid.as_str()) {
                   log_event(&format!("Authorized card: {}", uid))?;
                   control_led_and_buzzer(true)?;
                   enable_file_sharing()?;
               } else {
                   log_event(&format!("Unauthorized card: {}", uid))?;
                   control_led_and_buzzer(false)?;
                   println!("❗ Unauthorized card");
                   disable_file_sharing()?;
               }

               // Wait a moment before scanning again
               thread::sleep(Duration::from_secs(2));
           } else {
               // Small delay to prevent CPU usage spikes
               thread::sleep(Duration::from_millis(500));
           }
       }

       Ok(())
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
       let now = Local::now();
       let expiration = now + TimeDelta::try_minutes(TIMEOUT_MINUTES).unwrap();
       let expiration_str = expiration.format("%Y-%m-%d %H:%M:%S %z").to_string();
       
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
       
       // Disable the fileuser account
       let _ = Command::new("sudo")
           .args(["usermod", "-L", "fileuser"])
           .status()?;
       
       log_event("File sharing disabled")?;
       println!("\n❌ File sharing DISABLED");
       
       Ok(())
   }

   fn check_auth_state() -> io::Result<bool> {
       if !Path::new(AUTH_STATE).exists() {
           return Ok(false);
       }
       
       let expiration_str = fs::read_to_string(AUTH_STATE)?;
       
       match DateTime::parse_from_str(expiration_str.trim(), "%Y-%m-%d %H:%M:%S %z") {
           Ok(expiration) => {
               let now = Local::now();
               if now > expiration {
                   disable_file_sharing()?;
                   Ok(false)
               } else {
                   Ok(true)
               }
           },
           Err(_) => {
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

   fn control_led_and_buzzer(success: bool) -> io::Result<()> {
       // Control the ACR122U LED and buzzer based on authentication result
       // Uses a Python helper script since direct APDU commands are complex in Rust
       
       let command = if success {
           // Green LED on for 1 second with beep (success)
           "FF 00 40 0E 04 0A 0A 01 01"
       } else {
           // Red LED blink 3 times with beep (failure)
           "FF 00 40 D0 04 02 02 03 01"
       };
       
       let script = format!(r#"
   import sys
   from smartcard.System import readers
   from smartcard.util import toHexString

   try:
       r = readers()
       if len(r) == 0:
           sys.exit(1)
       
       connection = r[0].createConnection()
       connection.connect()
       
       cmd = [int(x, 16) for x in "{0}".split()]
       connection.transmit(cmd)
       
       connection.disconnect()
   except Exception:
       sys.exit(1)
   "#, command);
       
       // Execute the Python script
       let _ = Command::new("python3")
           .arg("-c")
           .arg(&script)
           .status();
       
       Ok(())
   }

   fn read_card_uid() -> Option<String> {
       // Use Python to read the card UID via PC/SC
       match Command::new("python3")
           .args(["-c", r#"
   import sys
   from smartcard.System import readers
   from smartcard.util import toHexString

   try:
       r = readers()
       if len(r) == 0:
           sys.exit(1)
       
       connection = r[0].createConnection()
       connection.connect()
       
       GET_UID = [0xFF, 0xCA, 0x00, 0x00, 0x00]
       response, sw1, sw2 = connection.transmit(GET_UID)
       
       if sw1 == 0x90 and sw2 == 0x00:
           print(toHexString(response))
       
       connection.disconnect()
   except Exception:
       sys.exit(1)
   "#])
           .output() {
           Ok(uid_output) => {
               let uid = String::from_utf8_lossy(&uid_output.stdout).trim().to_string();
               if !uid.is_empty() {
                   return Some(uid);
               }
               None
           },
           Err(_) => None
       }
   }
   ```

6. Build and run:
   ```bash
   cargo build --release
   sudo ./target/release/nfc-fileshare
   ```

7. To autostart, create a systemd service similar to the Python version but change the ExecStart line:
   ```
   ExecStart=/home/pi/rust-nfc-fileshare/target/release/nfc-fileshare
   ```

## Advanced NFC Card Features

You can enhance the system by using the ACR122U's additional capabilities:

### LED and Buzzer Control

The ACR122U reader has built-in red and green LEDs and a buzzer that can provide feedback:

1. The updated scripts already include basic LED/buzzer control:
   - Green LED with beep for successful authentication
   - Red blinking LED with beep for failed authentication

2. You can customize the LED patterns by modifying the APDU commands in the scripts:
   - In Python: Modify the `control_led_and_buzzer()` function
   - In Rust: Modify the `control_led_and_buzzer()` function

### Reading and Writing to NFC Cards

Beyond using just the card's UID, you can store data on supported cards:

1. Install additional tools:
   ```bash
   sudo apt install -y mfoc nfctools
   ```

2. Read data from a MIFARE Classic card:
   ```bash
   sudo mfoc -O card_dump.mfd
   ```

3. Write data to a card using Python:
   ```python
   # Example APDU command to write to a MIFARE Classic block (after authentication)
   WRITE_BLOCK = [0xFF, 0xD6, 0x00, BLOCK_NUMBER, 0x10] + [data_bytes]
   connection.transmit(WRITE_BLOCK)
   ```

4. You could enhance the file sharing system to:
   - Store user-specific permissions on cards
   - Keep access logs on the cards
   - Implement time-limited or usage-limited access

## Troubleshooting & FAQs

### Common Issues

1. **"Permission denied" when accessing the file_share directory**
   
   Solution:
   ```bash
   sudo chmod -R 755 /home/pi/file_share
   sudo chown -R pi:pi /home/pi/file_share
   ```

2. **Still able to access via SFTP when sharing is disabled**
   
   This usually means SFTP is using a different authentication method. The enhanced scripts in this guide fix this by creating and locking/unlocking a dedicated SFTP user. If you're still having issues:
   ```bash
   sudo usermod -L fileuser  # Lock the fileuser account
   ```

3. **ACR122U not detected**
   
   Check USB connection and try:
   ```bash
   sudo lsusb
   ```
   
   If it shows but doesn't work, try rebooting:
   ```bash
   sudo reboot
   ```

4. **"Unable to connect to card or no card in reader" error**
   
   Make sure you're placing the card properly on the reader. Some cards need to be placed in a specific position.

5. **Mobile device can't connect to the share**
   
   - Ensure both devices are on the same network
   - Check the Pi's firewall settings:
     ```bash
     sudo ufw status
     ```
   - If active, allow Samba and SSH:
     ```bash
     sudo ufw allow samba
     sudo ufw allow ssh
     ```

### Security Considerations

- The default timeout is 10 minutes. You can adjust the `TIMEOUT_MINUTES` variable in the script for longer or shorter durations.
- The default fileuser password is set to "password" in the Python script. Change this to something more secure.
- Consider changing the default Samba password for the "pi" user as well.
- Review the permissions on the file_share directory based on your security needs.

### Advanced Usage

- You can add multiple NFC cards to the `AUTHORIZED_UIDS` list.
- The scripts include built-in LED and buzzer control for user feedback.
- For industrial applications, consider implementing a more robust logging system.
- You can modify the scripts to support different access levels for different cards.
