# Pair a phone

You can pair your phone with your computer on the same network. Then the computer
can send tasks to the agent on the phone, or the other way round.

Pairing only lets the two devices recognize each other. It grants no permission.
Settings, keys and approvals stay on each device.

## Pair the devices

1. On the phone, open Settings > Devices and tap **Wait for a device**. It shows an
   address such as `192.168.0.3:47910`.
2. On the computer, open Settings > Devices, enter that address and press
   **Connect**. From a terminal you can instead run:

   ```powershell
   argentum device pair --connect 192.168.0.3:47910 --name Phone
   ```

3. Both devices show six digits. Confirm only when they match.

Paired devices are listed under Settings > Devices, each with a **Forget** button.
From a terminal, `argentum device list` and `argentum device forget FINGERPRINT`
do the same.

## Send a task to the phone

1. On the phone, turn on **Let paired devices control this app** under
   Settings > Devices. It shows the address it listens on, port 47911 by default.
2. On the computer, connect:

   ```powershell
   argentum device connect 192.168.0.3:47911
   ```

3. Type one JSON request per line, for example:

   ```json
   {"protocol_version":1,"request_id":"1","type":"command","command":{"kind":"submit_task","prompt":"Summarize README.md"}}
   ```

The run uses the phone's own provider, policy and approvals. A paired device can
send tasks, cancel a run and browse sessions. Changing settings, keys, permissions
or approvals is refused with `not_allowed_remotely`, and a paired device can never
choose Full Access.

The app has no screen for controlling another device yet, so use
`device connect` from a computer. The request format is the same as
[the JSONL server](jsonl-server.md).

More detail is in
[pair devices](https://github.com/AG064/argentum/blob/development/docs/CLI_GUIDE.md#pair-devices-and-control-one-from-another).
