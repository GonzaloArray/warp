# WhatsApp alerts (CallMeBot)

When a CLI agent finishes (`COMPLETED_UNSEEN`), Warp can send a WhatsApp message.

## 1. Create free CallMeBot link

1. Add **+34 644 59 71 67** (CallMeBot) in WhatsApp contacts.
2. Send: `I allow callmebot to send me messages`
3. You’ll get an **API key**.
4. Your phone in E.164 **without +**, e.g. `54911XXXXXXXX`.

## 2. Configure Warp OSS

Edit `~/.warp-oss/cli_agent_monitor/alerts.json`:

```json
{
  "desktop_enabled": true,
  "desktop_sound": true,
  "whatsapp_enabled": true,
  "whatsapp_phone": "54911XXXXXXXX",
  "whatsapp_api_key": "YOUR_KEY",
  "whatsapp_provider": "call_me_bot"
}
```

Or env (overrides file):

```bash
export WARP_WHATSAPP_ENABLED=1
export WARP_WHATSAPP_PHONE=54911XXXXXXXX
export WARP_WHATSAPP_API_KEY=YOUR_KEY
./target/debug/warp-oss
```

## 3. In the app

Open the agentes chip → panel footer:

- **✓ Alerta escritorio** — toggle macOS banner
- **○ WhatsApp** — toggle WhatsApp channel

## Notes

- Message example: `🔔 Sumanos / Warp` + title + body.
- Failures are logged (`CLI agent WhatsApp alert failed`).
- Restart Warp after editing `alerts.json` or use env before launch.
