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
