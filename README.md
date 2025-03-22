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
9. [Troubleshooting & FAQs](#troubleshooting--faqs)

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

1. Create the NFC file sharing script:
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

   def setup_samba():
       """Check if Samba is installed and configured"""
       try:
           result = subprocess.run(["systemctl", "status", "smbd"], 
                                  capture_output=True, text=True)
           if "active (running)" not in result.stdout:
               print("Samba is not running. Setting up Samba...")
               subprocess.run(["sudo", "apt", "install", "-y", "samba", "samba-common-bin"], 
                             check=True)
               
               # Create share directory if it doesn't exist
               os.makedirs(SHARE_PATH, exist_ok=True)
               
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
       except Exception as e:
           print(f"Error setting up Samba: {e}")
           log_event(f"Error setting up Samba: {e}")

   def enable_file_sharing():
       """Enable file sharing and set timeout"""
       # Create state file with expiration time
       expiration = datetime.now() + timedelta(minutes=TIMEOUT_MINUTES)
       with open(AUTH_STATE, "w") as state_file:
           state_file.write(expiration.strftime("%Y-%m-%d %H:%M:%S"))
       
       # Make the share directory accessible
       os.system(f"chmod -R 777 {SHARE_PATH}")
       
       # Also make it accessible locally
       os.system(f"sudo chown -R pi:pi {SHARE_PATH}")
       os.system(f"chmod -R 755 {SHARE_PATH}")
       
       log_event("File sharing enabled for 10 minutes")
       print(f"\n✅ File sharing ENABLED for {TIMEOUT_MINUTES} minutes")
       print(f"📁 Shared folder: {SHARE_PATH}")
       print(f"💻 Connect to: smb://{get_ip_address()}/FileShare")
       print(f"👤 Username: pi")
       print("⏱️  Timeout: " + expiration.strftime("%H:%M:%S"))

   def disable_file_sharing():
       """Disable file sharing"""
       if os.path.exists(AUTH_STATE):
           os.remove(AUTH_STATE)
       
       # Restrict permissions for network access
       os.system(f"chmod -R 700 {SHARE_PATH}")
       
       # But keep local access
       os.system(f"sudo chown -R pi:pi {SHARE_PATH}")
       os.system(f"chmod -R 755 {SHARE_PATH}")
       
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

   def handle_exit(signal, frame):
       """Handle exit gracefully"""
       print("\nShutting down NFC File Sharing service...")
       disable_file_sharing()
       sys.exit(0)

   def main():
       # Setup signal handlers
       signal.signal(signal.SIGINT, handle_exit)
       signal.signal(signal.SIGTERM, handle_exit)
       
       # Create directories if they don't exist
       os.makedirs(SHARE_PATH, exist_ok=True)
       
       # Check and setup Samba if needed
       setup_samba()
       
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
                       enable_file_sharing()
                   else:
                       log_event(f"Unauthorized card: {uid}")
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
   - Edit the nfc_fileshare.py script to update the AUTHORIZED_UIDS list:
     ```bash
     sudo nano /home/pi/nfc_fileshare.py
     ```
     Update the AUTHORIZED_UIDS list with your card's UID.

5. Run the file sharing script:
   ```bash
   sudo python3 /home/pi/nfc_fileshare.py
   ```

6. Test the system by placing your NFC card on the reader. The system should enable file sharing for 10 minutes.

## Connecting from Mobile Devices

### Using FX File Explorer

1. Install "FX File Explorer" from the Google Play Store on your Android device.

2. Set up SMB/CIFS connection:
   - Open FX File Explorer
   - Tap the "+" button (Add new storage)
   - Select "Network Storage"
   - Choose "Windows Share (SMB/CIFS)"
   - Enter the following details:
     - Name: Raspberry Pi
     - Server: [Your Pi's IP address] (shown in the terminal when file sharing is enabled)
     - Share: FileShare
     - Username: pi
     - Password: [Your Samba password]
   - Tap "Test" to verify the connection
   - Tap "Save" to add the storage

3. Transfer files:
   - Navigate to the files you want to transfer
   - Select the files using the selection tool
   - Tap "Copy" or "Move"
   - Navigate to your Raspberry Pi storage
   - Tap "Paste" to transfer the files

### Using SSH/SFTP Server

For an alternative file transfer method using SSH/SFTP:

1. Enable SSH on your Raspberry Pi (if not already enabled):
   ```bash
   sudo systemctl enable ssh
   sudo systemctl start ssh
   ```

2. Install an SFTP server app on your Android device:
   - "Solid Explorer File Manager" or "AndFTP" are good options

3. Set up SFTP connection:
   - Open your SFTP app
   - Add a new connection with these details:
     - Protocol: SFTP
     - Host/Server: [Your Pi's IP address]
     - Port: 22
     - Username: pi
     - Password: [Your Raspberry Pi password]
     - Initial directory: /home/pi/file_share

4. Connect and transfer files.

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

For better performance, you can use the Rust implementation:

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
   
   (Copy and paste the Rust implementation code)

6. Build and run:
   ```bash
   cargo build --release
   sudo ./target/release/nfc-fileshare
   ```

7. To autostart, create a systemd service similar to the Python version but change the ExecStart line:
   ```
   ExecStart=/home/pi/rust-nfc-fileshare/target/release/nfc-fileshare
   ```

## Troubleshooting & FAQs

### Common Issues

1. **"Permission denied" when accessing the file_share directory**
   
   Solution:
   ```bash
   sudo chmod -R 755 /home/pi/file_share
   sudo chown -R pi:pi /home/pi/file_share
   ```

2. **ACR122U not detected**
   
   Check USB connection and try:
   ```bash
   sudo lsusb
   ```
   
   If it shows but doesn't work, try rebooting:
   ```bash
   sudo reboot
   ```

3. **"Unable to connect to card or no card in reader" error**
   
   Make sure you're placing the card properly on the reader. Some cards need to be placed in a specific position.

4. **Mobile device can't connect to the share**
   
   - Ensure both devices are on the same network
   - Check the Pi's firewall settings:
     ```bash
     sudo ufw status
     ```
   - If active, allow Samba:
     ```bash
     sudo ufw allow samba
     ```

5. **"Unable to transmit data. (TX)" or similar errors**
   
   This usually indicates a driver conflict. Make sure you've created the blacklist file and rebooted:
   ```bash
   sudo nano /etc/modprobe.d/blacklist-nfc.conf
   ```
   
   Add:
   ```
   blacklist pn533
   blacklist pn533_usb
   blacklist nfc
   ```
   
   Then reboot:
   ```bash
   sudo reboot
   ```

### Security Considerations

- The default timeout is 10 minutes. You can adjust the `TIMEOUT_MINUTES` variable in the script for longer or shorter durations.
- Consider changing the default Samba password to something more secure.
- Review the permissions on the file_share directory based on your security needs.

### Advanced Usage

- You can add multiple NFC cards to the `AUTHORIZED_UIDS` list.
- Consider implementing encryption for more sensitive data.
- For industrial applications, consider implementing a more robust logging system.
