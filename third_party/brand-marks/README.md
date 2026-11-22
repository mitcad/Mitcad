# Third-party marks

Logos of other organisations that MitCad shows where it links to them. They
are trademarks of their owners, not MitCad's code: the MIT licence does not
cover them, and they are used only as their owners' brand guidelines allow
(docs/development.md, "Licence policy"; docs/ui-conventions.md section 11
names the buttons that may show one).

## discord-mark.svg

Discord's logo mark (the "Clyde" symbol), white, on the top bar's Discord
pill (mitcad#2159, `app/framework/DiscordButton.cpp`; app/COMMANDS.md
"Discord"), which opens MitCad's Discord server.

- Source: the Symbol asset of Discord's brand page,
  https://discord.com/branding (the page's `Symbol.svg`,
  https://cdn.prod.website-files.com/6257adef93867e50d84d30e2/66e3d7f4ef6498ac018f2c55_Symbol.svg),
  taken on 21.11.2026, unchanged. SHA-256 of the file:
  `eb3bf5a2e8b9f9aa39c1b1d38aeaa7c2997084567634c55f4141aa39cf426cec`.
- Owner: Discord Inc.; "Discord" and the mark are its trademarks.
- Terms: Discord's Brand Guidelines on that page. They allow the marks "to
  inform people that you are a Discord user and/or that you have a Discord
  server", on digital assets only, without implying that the user is
  Discord or sponsored by it; the logo may be used "in color, black or
  white", and it must not be edited, changed, distorted, recoloured or
  reconfigured.
- How MitCad keeps to them: the mark is drawn whole, in its own
  proportions (64 × 48), in white on Discord's blurple `#5865F2`
  (`ColorRole::DiscordMark` on `ColorRole::DiscordFill`), only on the button
  that links to MitCad's own server, with the tooltip "Opens MitCad's
  Discord server"; it is no command icon and is not used elsewhere. It is
  built into the application as the Qt resource `:/brand/discord-mark.svg`
  (`app/CMakeLists.txt`).
