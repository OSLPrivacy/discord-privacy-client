# OSL Enclaves, Beacons, and Roles: best-of source-marked spec

Status: design input for the Enclaves build. This file is not a shipped-product
claim. Every merged setting row carries at least one source mark from the four
sweeps requested by Task 4850: DISCORD, TELEGRAM, MODS, and COMPLAINTS.

Terminology:

- Enclave: an OSL-owned community container with channels, topics, roles, member
  admission, moderation state, and encrypted delivery policy.
- Beacon: an intentional broadcast or attention surface, such as an announcement,
  rule update, event notice, join alert, moderator alert, digest, or resource pin.
- Role: a delegated authority or identity tag that grants capabilities, changes
  visibility, or explains who is speaking without exposing extra private activity.

## Source section: DISCORD

Sweep source mark: DISCORD.

- Discord Community Onboarding lets members answer questions to receive channels
  and roles, exposes a Channels & Roles tab after joining, requires desktop admin
  setup, and recommends previewing role/channel assignment before publishing:
  <https://support.discord.com/hc/en-us/articles/11074987197975-Community-Onboarding-FAQ>
- Discord roles rely on top-down hierarchy, the Administrator permission bypasses
  channel restrictions, Manage Roles can affect only lower roles, and role-exclusive
  channels remove broad visibility before adding explicit access:
  <https://support.discord.com/hc/en-us/articles/214836687-Discord-Roles-and-Permissions>
- Discord server setup groups owner-facing controls around roles, member pages,
  verification, pruning, insights, raid protection, rules, onboarding, and welcome
  screens:
  <https://support.discord.com/hc/en-us/articles/33023827550359-Discord-Server-Setup-Guide>
- Discord Rules Screening and Safety Setup centralize rules, verification level,
  and AutoMod configuration:
  <https://support.discord.com/hc/en-us/articles/1500000466882-Rules-Screening-FAQ>
- Discord View Server As Role makes permission review explicit for owners before
  changes affect members:
  <https://support.discord.com/hc/en-us/articles/360055709773-View-Server-As-Role-Permission-Guide>

## Source section: TELEGRAM

Sweep source mark: TELEGRAM.

- Telegram FAQ describes granular administrator privileges, mass delete,
  membership control, pinned messages, aggressive anti-spam, invite links, direct
  messages to channels, and suggested posts:
  <https://telegram.org/faq>
- Telegram API rights describe granular admin rights, default member rights,
  banned rights, invite links, join requests, forum topics, discussion groups,
  statistics, and admin logs:
  <https://core.telegram.org/api/rights>
- Telegram chatAdminRights enumerates channel and supergroup administrator rights
  and connects them to admin logs, statistics, topics, stories, direct messages,
  suggested posts, and member tags:
  <https://core.telegram.org/constructor/chatAdminRights>
- Telegram group permissions allow admins to restrict media types or all member
  posting, while slow mode throttles high-volume groups:
  <https://telegram.org/blog/permissions-groups-undo> and
  <https://telegram.org/blog/silent-messages-slow-mode>
- Telegram topics, reactions, member tags, channel boosts, and official apps show
  how communities expose discussion structure, role labels, reaction policy, and
  client diversity:
  <https://telegram.org/blog/topics-in-groups-collectible-usernames>,
  <https://telegram.org/blog/reactions-spoilers-translations>,
  <https://telegram.org/blog/member-tags-disable-sharing-and-more>,
  <https://telegram.org/blog/channel-stories>, and
  <https://telegram.org/apps>

## Source section: MODS

Sweep source mark: MODS.

- Dyno advertises configurable moderation, mod logs, timed mutes/bans,
  auto-moderation, anti-spam, auto roles, and custom commands:
  <https://dyno.gg/bot> and <https://docs.dyno.gg/en/modules/automod>
- Carl-bot describes a modular Discord bot with reaction roles, automod, logging,
  custom commands, suggestions, autoroles, embeds, starboard, feeds, reminders,
  triggers, and repeated messages:
  <https://carl.gg/>
- MEE6 positions itself as an all-in-one Discord role, moderation, reaction-role,
  leveling, custom command, and welcome-message bot:
  <https://mee6.xyz/en/> and <https://mee6.xyz/en/tutorials>
- Vencord and BetterDiscord show recurring client-mod demand for plugins, themes,
  custom CSS, privacy controls, server info, role viewing, timestamp tools,
  message actions, notification controls, and local-only UI extensions:
  <https://vencord.dev/>, <https://vencord.dev/plugins>, and
  <https://betterdiscord.app/>
- Telegram X, Nekogram, and Unigram show demand for alternate Telegram clients
  with TDLib speed, experimental features, appearance controls, chat utilities,
  translation, keyboard shortcuts, and Windows-native integration:
  <https://telegram.org/apps>, <https://nekogram.app/>, and
  <https://github.com/unigramdev/unigram>

## Source section: COMPLAINTS

Sweep source mark: COMPLAINTS.

- Discord support/community complaints around Server Guide and onboarding report
  resource channels becoming hard to edit, rules and resources being hard to find
  after migration, failed saves, desktop/web inconsistency, and hidden read-only
  resource pages:
  <https://support.discord.com/hc/en-us/community/posts/24886597041047-Edit-content-on-Resources-channels-from-onboarding-feature-is-quite-cumbersome> and
  <https://support.discord.com/hc/en-us/community/posts/14430433889815-Onboarding-Issues-Can-t-change-previously-set-Rules-Can-t-change-Resources>
- Discord support/community complaints around onboarding include irreversible or
  unclear Server Guide state, resource-page access confusion, and Channels & Roles
  being useful but not addressable as a direct mention target:
  <https://support.discord.com/hc/en-us/community/posts/13822214115991-Having-access-to-turn-off-Server-Guide> and
  <https://support.discord.com/hc/en-us/community/posts/16815195387799-Mention-Tag-the-new-Channels-Roles-option-in-messages>
- Telegram Bugs and Suggestions complaints around slow mode report unclear member
  messaging, accidental slider activation, media album friction, and discussion
  group permission toggles that appeared changeable but did not save:
  <https://bugs.telegram.org/c/5596>,
  <https://bugs.telegram.org/c/7174>,
  <https://bugs.telegram.org/c/3630>, and
  <https://bugs.telegram.org/?query=discussion+permissions&sort=time>
- Third-party client and bot discussions surface recurring needs for permission
  diagnosis, role visibility, command visibility, reaction-role reliability,
  privacy controls, local customization, and caution around plugin/client trust:
  <https://vencord.dev/plugins>,
  <https://betterdiscord.app/>,
  <https://www.reddit.com/r/Mee6/comments/1onkufz/how_to_set_up_the_bot_for_my_moderators/>, and
  <https://www.reddit.com/r/Discord_Bots/comments/1dz6xdg/carl_bot_reaction_roles_not_working/>

## Merged settings rows

| ID | Domain | Setting | Best-of requirement for OSL | Source marks | Notes |
|---|---|---|---|---|---|
| BS-001 | Enclave identity | Enclave name | Require a stable display name, immutable internal ID, optional short handle, and visible owner profile before any member can be invited. | DISCORD; TELEGRAM | Discord server identity and Telegram public chat usernames both separate public labels from internal authority. |
| BS-002 | Enclave identity | Enclave type | Support private, invite-only, public-discoverable, and announcement-only Enclave modes as policy presets, not as separate products. | DISCORD; TELEGRAM | Discord friend/community split and Telegram group/channel split both map to preset intent. |
| BS-003 | Enclave identity | Owner role | Preserve one cryptographic owner authority that can transfer ownership only through an explicit signed ceremony. | DISCORD; TELEGRAM; COMPLAINTS | Owner loss and creator deletion complaints make transfer and recovery explicit requirements. |
| BS-004 | Enclave identity | Admin profile | Require each admin to have a visible role label, capability summary, and last governance acknowledgement. | TELEGRAM; DISCORD | Telegram admin titles and Discord role colors both explain who speaks with authority. |
| BS-005 | Enclave identity | Community language | Store primary language on the Enclave and allow channel-level language hints for discovery and moderation routing. | DISCORD; TELEGRAM | Discord community language and Telegram global communities both need language hints. |
| BS-006 | Enclave identity | Rules home | Every Enclave has one canonical rules resource with version, signer, effective time, and member acceptance state. | DISCORD; COMPLAINTS | Rules Screening and onboarding complaints require a single editable source of truth. |
| BS-007 | Enclave identity | Public description | Publish a short public description separately from member-only resources, and preview it before invite links are created. | DISCORD; TELEGRAM | Discord welcome and Telegram public chat previews both influence join expectations. |
| BS-008 | Enclave identity | Resource catalog | Model rules, FAQs, guides, and policies as editable resources, not hidden channels masquerading as pages. | DISCORD; COMPLAINTS | Discord Server Guide complaints show resource pages must stay directly editable. |
| BS-009 | Enclave identity | Availability claim | Mark unavailable features as unavailable inside settings so owners cannot imply shipped voice, stories, or discovery modes. | DISCORD; TELEGRAM; COMPLAINTS | Both ecosystems expose evolving feature sets that need honest state. |
| BS-010 | Enclave identity | Preview mode | Owners and admins can preview the Enclave as any role bundle before publishing a setting change. | DISCORD; MODS; COMPLAINTS | Discord View Server As Role and mod permission viewers show preview demand. |
| BS-011 | Enclave identity | Change journal | Every owner-facing setting writes a human-readable governance event to the Enclave admin log. | TELEGRAM; DISCORD; MODS | Telegram admin logs and Discord logs establish auditability as owner hygiene. |
| BS-012 | Enclave identity | Recovery contacts | Owners can designate recovery admins with limited signed rescue actions, visible to members before they are needed. | TELEGRAM; COMPLAINTS | Ownership-transfer and admin-rights complaints require predeclared recovery paths. |
| BS-013 | Channels | Channel kind | Support text, resource, announcement, topic/forum, voice-placeholder, mod-log, ticket, and bot-only channel kinds. | DISCORD; TELEGRAM; MODS | Discord channels, Telegram topics, and bot ecosystems define the practical set. |
| BS-014 | Channels | Category policy | Categories may inherit permissions, but every channel shows whether it is synced or has local overrides. | DISCORD; COMPLAINTS | Discord channel/category permission confusion makes override visibility mandatory. |
| BS-015 | Channels | Topic forums | Large Enclaves can enable forum topics with per-topic lock, archive, slow mode, and moderator ownership. | TELEGRAM; DISCORD | Telegram forum topics and Discord forums converge on topic-level organization. |
| BS-016 | Channels | Closed topics | Closing a topic blocks normal member posting while preserving moderator/admin posting and read access rules. | TELEGRAM; DISCORD; COMPLAINTS | Telegram topic complaints show owners expect announcement-like locked topics. |
| BS-017 | Channels | Resource channels | Resource channels stay editable from settings and from the normal channel tree even when shown in onboarding or guide surfaces. | DISCORD; COMPLAINTS | Discord resource-page complaints explicitly object to hidden edit paths. |
| BS-018 | Channels | Default channels | New-member default channels must be low-risk, high-signal, and removable from the default set without deleting the channel. | DISCORD; COMPLAINTS | Discord onboarding recommends useful defaults and complaints object to sticky state. |
| BS-019 | Channels | Hidden channels | Hidden channels must not reveal names, unread counts, topic titles, or membership through normal member APIs. | DISCORD; MODS | Role-exclusive channel patterns and hidden-channel mods make this explicit. |
| BS-020 | Channels | Read-only mode | Owners can make any channel read-only for members while preserving moderator posting and beacon publishing. | DISCORD; TELEGRAM | Discord announcements and Telegram channels both need read-only community surfaces. |
| BS-021 | Channels | Media policy | Each channel can allow or deny images, videos, links, files, stickers, GIFs, voice notes, polls, and embeds separately. | TELEGRAM; DISCORD; MODS | Telegram group permissions and bot automod both use media-type controls. |
| BS-022 | Channels | File limits | Enclave file policy stores maximum size, allowed MIME groups, scan state, retention policy, and beacon eligibility. | TELEGRAM; MODS | Telegram upload-size expectations and bot filters make file policy visible. |
| BS-023 | Channels | Link policy | Channels can require link allowlists, deny masked links, strip trackers, or route first-time domains to moderator review. | DISCORD; MODS; COMPLAINTS | AutoMod, Vencord URL cleanup, and suspicious-link friction motivate this. |
| BS-024 | Channels | Mentions | Channel policy defines who can mention everyone, here, roles, groups, admins, and beacons. | DISCORD; TELEGRAM; COMPLAINTS | Discord mention controls and Telegram requests for all/admin mentions shape this. |
| BS-025 | Channels | Channel directory | Members get a Browse Channels view that shows only permitted channels plus owner-written descriptions for optional channels. | DISCORD; COMPLAINTS | Discord Channels & Roles and complaints about discoverability require this. |
| BS-026 | Channels | Channel templates | Owners can duplicate channel permission recipes and apply them to categories without copying stale member exceptions silently. | DISCORD; MODS | Discord duplicate-channel workflows and bot template demand support this. |
| BS-027 | Channels | Channel health | Settings flag channels with no owner, no recent moderator review, conflicting overrides, or unassigned onboarding state. | DISCORD; COMPLAINTS | Onboarding assignment warnings and resource confusion require diagnostics. |
| BS-028 | Channels | Channel archival | Archive preserves encrypted history, role access policy, resource references, and beacon log without accepting new posts. | DISCORD; TELEGRAM | Forum/topic archiving and pinned resources both need durable history. |
| BS-029 | Membership | Invite links | Invite links are named, scoped, expirable, revocable, rate-limited, and optionally require join requests. | TELEGRAM; DISCORD | Telegram invite links and Discord invites require owner-level lifecycle controls. |
| BS-030 | Membership | Join requests | Owners can require approval, delegate approval to a role, and store signed reason codes for approval or denial. | TELEGRAM; DISCORD; MODS | Telegram join requests and moderation bots both use approval workflows. |
| BS-031 | Membership | Screening | Enclave screening combines rules acceptance, capability disclosure, optional questions, and anti-raid protections. | DISCORD; TELEGRAM | Discord Rules Screening and Telegram anti-spam guide the baseline. |
| BS-032 | Membership | Onboarding questions | Questions can assign roles, channels, notification groups, and beacons, with required and multi-select variants. | DISCORD; MODS | Discord onboarding and reaction-role bots converge on choice-based assignment. |
| BS-033 | Membership | Onboarding scope | Owners must map most member-visible channels through default channels or onboarding choices before publication. | DISCORD; COMPLAINTS | Discord warns unassigned channels may be missed by new members. |
| BS-034 | Membership | Onboarding preview | Preview shows exact roles, channels, beacons, notification defaults, and blocked actions for every answer set. | DISCORD; MODS; COMPLAINTS | Discord preview and bot permission issues make dry-run assignment mandatory. |
| BS-035 | Membership | Rejoin behavior | Owners choose whether rejoining members repeat screening, regain prior roles, or enter a review queue. | DISCORD; TELEGRAM | Discord rejoin onboarding and Telegram invite controls imply explicit policy. |
| BS-036 | Membership | Default role | The default member role must be minimal, visible in policy, and unable to post broadly until screening policy allows it. | DISCORD; TELEGRAM | Discord @everyone and Telegram default rights both define baseline member power. |
| BS-037 | Membership | Member page | Admins can inspect members, roles, join source, screening state, restrictions, timeout state, and last moderation action. | DISCORD; TELEGRAM; MODS | Discord Members Page, Telegram recent actions, and bot logs converge here. |
| BS-038 | Membership | Pruning | Owners can identify inactive members, preview affected access, and remove them with a reversible grace marker. | DISCORD; COMPLAINTS | Discord pruning and complaints about accidental state loss require preview. |
| BS-039 | Membership | Guest access | Temporary guests can receive channel-limited, time-limited access without joining full Enclave membership. | TELEGRAM; DISCORD | Telegram guest bot direction and Discord invite use cases suggest scoped guests. |
| BS-040 | Membership | Bot members | Bots have distinct member records with owner, scopes, message visibility, command visibility, and revocation. | TELEGRAM; MODS; COMPLAINTS | Telegram bot privacy mode and Discord bot command complaints require clarity. |
| BS-041 | Membership | Device roster | Member device participation is private by default; admin views show only trust/ack status needed for delivery safety. | TELEGRAM; DISCORD | Both platforms expose multi-device use but OSL must avoid presence expansion. |
| BS-042 | Membership | Member tags | Owners can enable self-set member tags, admin-assigned tags, or no tags, with anti-impersonation rules. | TELEGRAM; DISCORD; MODS | Telegram member tags and Discord roles/colors both serve identity hints. |
| BS-043 | Membership | Pronouns and labels | Optional profile labels must be member-controlled unless a role grants an official badge. | MODS; DISCORD; TELEGRAM | Client mods show demand for pronoun labels; role systems need authenticity. |
| BS-044 | Membership | Account age gates | Screening can require account age, invite trust, device trust, or manual approval without exposing those signals to peers. | DISCORD; TELEGRAM; MODS | Verification levels, anti-raid tools, and anti-spam controls inform this. |
| BS-045 | Roles | Role hierarchy | Roles have ordered management authority, but read access and moderation authority are separate grants. | DISCORD; TELEGRAM | Discord role hierarchy and Telegram granular admin rights require separation. |
| BS-046 | Roles | Administrator | Administrator is a named break-glass capability, off by default, logged, and visually distinguished from routine moderator roles. | DISCORD; TELEGRAM; COMPLAINTS | Discord warns Administrator is most powerful; Telegram full rights need care. |
| BS-047 | Roles | Manage roles | A role can manage only lower roles and only grant capabilities it already has or is explicitly allowed to delegate. | DISCORD; TELEGRAM | Discord Manage Roles rules and Telegram admin rights align here. |
| BS-048 | Roles | Role-exclusive access | Private role channels remove default visibility first, then add explicit role or member grants. | DISCORD; TELEGRAM | Discord role-exclusive channels and Telegram private groups/channels require deny-first access. |
| BS-049 | Roles | Role colors | Role colors and badges indicate visible identity only; they never imply hidden privileges unless the member can inspect them. | DISCORD; TELEGRAM; MODS | Discord role colors and Telegram admin tags are useful but can mislead. |
| BS-050 | Roles | Custom titles | Admin and member titles are separate from permissions and must display their assignment authority. | TELEGRAM; DISCORD | Telegram custom admin titles and Discord role labels require clarity. |
| BS-051 | Roles | Moderator role | Moderator bundles are presets made from explicit permissions, not a magic class; owners can inspect every included capability. | DISCORD; TELEGRAM; MODS | Discord mods, Telegram admins, and moderation bots all need explicit grants. |
| BS-052 | Roles | Bot master role | Bot configuration authority must not require full admin; it grants bot settings only and is visible in integrations. | MODS; DISCORD; COMPLAINTS | MEE6 slash-command permission complaints show bot-master overgrant risk. |
| BS-053 | Roles | Command visibility | Slash commands and bot commands show only when the member has both command permission and channel permission. | MODS; DISCORD; COMPLAINTS | Bot command visibility complaints require unified permission resolution. |
| BS-054 | Roles | Role assignment source | Each role assignment records source: owner, admin, onboarding answer, reaction/button, import, or bot action. | DISCORD; TELEGRAM; MODS | Onboarding, admin rights, and reaction-role bots all assign roles. |
| BS-055 | Roles | Self-assignable roles | Owners can mark roles self-assignable with limits, dependencies, conflicts, and removal rules. | DISCORD; MODS | Discord Channels & Roles and reaction-role bots set user expectation. |
| BS-056 | Roles | Role conflicts | Mutually exclusive roles are enforced at assignment time and reported before publishing onboarding choices. | DISCORD; MODS; COMPLAINTS | Onboarding preview and reaction-role reliability require conflict checks. |
| BS-057 | Roles | Role dependencies | A role can require another role, screening completion, age gate, or owner approval before activation. | DISCORD; TELEGRAM; MODS | Advanced server setup and join approval imply staged capabilities. |
| BS-058 | Roles | Role mention policy | Each role stores whether it is mentionable, by whom, at what rate, and whether mentions become beacons. | DISCORD; TELEGRAM; COMPLAINTS | Mention noise and all/admin mention requests shape beacon policy. |
| BS-059 | Roles | Role audit | Role create, delete, reorder, grant, revoke, and permission edits are logged with actor, target, before, and after. | TELEGRAM; DISCORD; MODS | Telegram admin log and bot mod logs make role audit non-negotiable. |
| BS-060 | Roles | Role preview | Role detail pages show effective permissions, visible channels, manageable roles, beacons, and integrations. | DISCORD; MODS; COMPLAINTS | View-as-role and permission viewer mods expose this need. |
| BS-061 | Roles | Role import | Imported Discord/Telegram roles arrive as disabled drafts until owners confirm hierarchy, channel access, and beacon rights. | DISCORD; TELEGRAM; COMPLAINTS | Platform migrations are error-prone and need no silent grants. |
| BS-062 | Roles | Role removal | Removing a role previews lost channels, pending beacons, bot commands, scheduled tasks, and moderation queues. | DISCORD; TELEGRAM; MODS | Role deletion affects many surfaces and must be dry-run safe. |
| BS-063 | Moderation | Auto moderation | Auto moderation supports keyword lists, spam thresholds, mention floods, link filters, media filters, and repeated-message filters. | DISCORD; TELEGRAM; MODS | Discord AutoMod, Telegram anti-spam, Dyno, and Carl-bot define baseline controls. |
| BS-064 | Moderation | Auto actions | Auto actions include block, warn, delete, timeout, restrict media, require CAPTCHA, queue for review, and alert moderators. | DISCORD; TELEGRAM; MODS | Discord raid protection and bot automod use graduated actions. |
| BS-065 | Moderation | Moderation log | Every moderation action has a private log channel with action type, actor, reason, affected content hash, and appeal state. | TELEGRAM; DISCORD; MODS | Telegram recent actions, Discord audit logs, and bot mod logs converge. |
| BS-066 | Moderation | Public mod notes | Owners can publish coarse rule-enforcement beacons without exposing private evidence or member reports. | DISCORD; TELEGRAM; COMPLAINTS | Communities need transparency without creating harassment targets. |
| BS-067 | Moderation | Timeout | Timeout suspends posting, reacting, beacon triggering, and invite creation while preserving read access if policy allows. | DISCORD; TELEGRAM; MODS | Discord timeout and Telegram restrictions map to capability-specific suspension. |
| BS-068 | Moderation | Restricted media | Member restrictions can disable specific media classes instead of banning a member from all participation. | TELEGRAM; MODS | Telegram group permissions and moderation bots support granular restriction. |
| BS-069 | Moderation | Anti-raid | Join-rate, account-age, invite-source, spam-wave, and coordinated-behavior signals can trigger temporary join friction. | DISCORD; TELEGRAM; MODS | Discord raid protection and Telegram aggressive anti-spam inform this. |
| BS-070 | Moderation | CAPTCHA gate | CAPTCHA is a temporary join friction action with start/end time, reason, and owner-visible false-positive count. | DISCORD; COMPLAINTS | Discord raid protection uses CAPTCHA and owners need clear state. |
| BS-071 | Moderation | Appeal queue | Kicks, bans, timeouts, and heavy restrictions can create appeal records with owner-configurable deadlines. | DISCORD; TELEGRAM; MODS | Modmail/ticket bot patterns make appeals a first-class workflow. |
| BS-072 | Moderation | Mass moderation | Admins can select multiple messages or users and apply multiple moderation actions with a preview and undo window. | TELEGRAM; MODS; DISCORD | Telegram mass moderation and bots prove demand for bulk workflows. |
| BS-073 | Moderation | Report intake | Member reports route to a private queue with deduplication, reporter privacy, target context, and escalation role. | DISCORD; TELEGRAM; MODS | Server moderation and ticket systems require structured intake. |
| BS-074 | Moderation | Moderator roles | Mod roles can be protected from lower-role kicks, bans, timeouts, or role edits. | DISCORD; MODS | Discord hierarchy and bot protected-role settings require this. |
| BS-075 | Moderation | Protected members | Owners can protect selected members or roles from automated destructive actions unless two admins approve. | MODS; DISCORD; COMPLAINTS | Anti-nuke and protected-role bot patterns reduce automation damage. |
| BS-076 | Moderation | Rule matching | Keyword rules support exact, normalized, regex-like safe patterns, context windows, exception lists, and test mode. | DISCORD; MODS; COMPLAINTS | AutoMod and automod bots require tuning without surprising false positives. |
| BS-077 | Moderation | Spam thresholds | Spam rules separate repeated text, repeated media, mention floods, link floods, emoji floods, and rapid join/post behavior. | DISCORD; TELEGRAM; MODS | Common bot filters and platform anti-spam controls use separate signals. |
| BS-078 | Moderation | Moderator alerts | Alerts are beacons that can route by severity, channel, role, and quiet hours, with acknowledgement tracking. | DISCORD; TELEGRAM; MODS | Discord activity alerts and bot mod logs map moderation to beacon flow. |
| BS-079 | Moderation | Evidence retention | Moderation evidence retention is separate from message retention and shows cryptographic hashes when plaintext is removed. | DISCORD; TELEGRAM; COMPLAINTS | Encrypted communities need audit without unnecessary retention. |
| BS-080 | Moderation | False positive review | Owners can mark auto actions as false positive, update rules, and measure recurring bad filters. | DISCORD; MODS; COMPLAINTS | AutoMod tuning complaints require feedback loops. |
| BS-081 | Beacons | Announcement beacon | Announcement beacons are signed, channel-scoped broadcasts that support title, body, priority, audience, and expiry. | DISCORD; TELEGRAM | Discord announcements and Telegram channels define one-way broadcast needs. |
| BS-082 | Beacons | Rules beacon | Rules changes send a mandatory beacon to affected members with old version, new version, effective time, and required acknowledgement. | DISCORD; TELEGRAM; COMPLAINTS | Rules Screening and resource-edit complaints require explicit change notice. |
| BS-083 | Beacons | Join beacon | Join beacons are configurable by channel and role, and can be silent, public, moderator-only, or disabled. | DISCORD; TELEGRAM; MODS | Welcome bots and group service messages shape join alert policy. |
| BS-084 | Beacons | Leave beacon | Leave/removal beacons route differently for voluntary leave, kick, ban, prune, and ownership transfer. | DISCORD; TELEGRAM; MODS | Mod logs and membership events need accurate semantics. |
| BS-085 | Beacons | Admin alert beacon | Suspicious join waves, moderation backlog, failed saves, bot permission drift, and stalled resource edits create admin beacons. | DISCORD; MODS; COMPLAINTS | Discord alerts and complaints show admins need operational beacons. |
| BS-086 | Beacons | Mention beacon | Mentions of everyone, admins, roles, or topic owners are treated as beacons with rate limits and permission checks. | DISCORD; TELEGRAM; COMPLAINTS | Mention controls and requests for all/admin mentions require formal routing. |
| BS-087 | Beacons | Event beacon | Events support scheduled time, RSVP, reminders, cancellation, calendar export, and role-scoped notification. | DISCORD; MODS | Discord event bots and scheduled announcements drive this. |
| BS-088 | Beacons | Resource beacon | Resource updates can notify only members whose roles or channels are affected, avoiding server-wide noise. | DISCORD; COMPLAINTS | Server Guide resource friction motivates targeted notices. |
| BS-089 | Beacons | Digest beacon | Owners can configure daily or weekly digests for muted channels, unread highlights, rule changes, and admin actions. | DISCORD; TELEGRAM; MODS | Busy communities need summaries without constant pings. |
| BS-090 | Beacons | Silent beacon | Any beacon can be silent: it appears in logs and inboxes without push sound unless policy makes it mandatory. | TELEGRAM; DISCORD | Telegram silent messages and Discord notification controls inform this. |
| BS-091 | Beacons | Beacon acknowledgement | Mandatory beacons track member acknowledgement without exposing read receipts to other members. | DISCORD; TELEGRAM; COMPLAINTS | Rules acknowledgement and privacy limits both matter. |
| BS-092 | Beacons | Beacon permissions | Beacon creation, scheduling, editing, cancellation, and acknowledgement export are separate role capabilities. | DISCORD; TELEGRAM; MODS | Admin rights and bot scheduled messages need separate grants. |
| BS-093 | Beacons | Beacon channels | Every beacon has a destination channel and optional inbox mirror; hidden channels never leak through beacon titles. | DISCORD; TELEGRAM; MODS | Announcement routing must respect channel visibility. |
| BS-094 | Beacons | Beacon archive | Beacon archive is searchable by role, topic, sender, effective date, and source setting change. | TELEGRAM; DISCORD; MODS | Pinned messages, logs, and resource pages define expected retrieval. |
| BS-095 | Discovery | Welcome surface | New members see a concise welcome surface with rules, first actions, default channels, and how to change choices. | DISCORD; TELEGRAM | Discord Server Guide and Telegram pinned messages inform welcome flow. |
| BS-096 | Discovery | To-do list | Owners can define 3 to 5 first tasks that grant no hidden permissions and can be completed or dismissed explicitly. | DISCORD; COMPLAINTS | Discord Server Guide tasks need clear, non-sticky state. |
| BS-097 | Discovery | Browse channels | Members can browse optional channels they are allowed to join, filtered by role, topic, language, and activity. | DISCORD; MODS | Discord Browse Channels and client mods show navigation demand. |
| BS-098 | Discovery | Browse roles | Members can browse self-assignable roles with descriptions, conflicts, dependencies, and channel/beacon effects. | DISCORD; MODS; COMPLAINTS | Channels & Roles and reaction-role bots require transparency. |
| BS-099 | Discovery | Search resources | Resource search includes rules, FAQs, pinned beacons, admin posts, and public channel descriptions. | DISCORD; TELEGRAM; MODS | Server Guide, pinned messages, and client search improvements converge. |
| BS-100 | Discovery | Pinned messages | Channels and topics can pin multiple messages with pin owner, expiry, target roles, and beacon conversion. | TELEGRAM; DISCORD | Telegram pins and Discord resources both use persistent guidance. |
| BS-101 | Discovery | Recommended channels | Owners can recommend up to five channels per audience segment and preview recommendation collisions. | DISCORD; COMPLAINTS | Discord welcome screen recommendations and onboarding overload complaints shape this. |
| BS-102 | Discovery | Deep links | Settings can generate deep links to channels, roles, resources, beacons, and onboarding choices. | DISCORD; TELEGRAM; COMPLAINTS | Complaints about unmentionable Channels & Roles make deep links necessary. |
| BS-103 | Discovery | Unavailable states | Disabled, archived, hidden, not-yet-built, and permission-denied states have distinct copy and no misleading actions. | DISCORD; TELEGRAM; COMPLAINTS | Server Guide and evolving Telegram feature confusion require precise state. |
| BS-104 | Discovery | Mobile parity | Every member-facing setting has mobile-readable state; every owner-only desktop setting declares that it is desktop-only. | DISCORD; TELEGRAM; COMPLAINTS | Discord onboarding desktop-only setup and Telegram multi-client use require this. |
| BS-105 | Integrations | Bot install | Bot install requires owner approval, requested scopes, default admin rights, visible data access, and revoke path. | TELEGRAM; DISCORD; MODS | Telegram bot rights and Discord bot ecosystems require scoped installs. |
| BS-106 | Integrations | Bot privacy | Bots must declare whether they can see all messages, commands only, service messages, or channel posts. | TELEGRAM; MODS; COMPLAINTS | Telegram bot privacy mode is a recurring source of confusion. |
| BS-107 | Integrations | Bot commands | Command registration includes role, channel, slash/menu visibility, cooldowns, and audit log entries. | DISCORD; MODS; COMPLAINTS | MEE6 command visibility complaints require integration-level command sync. |
| BS-108 | Integrations | Reaction roles | Reaction/button role assignment is a native workflow with idempotency, permission checks, and recovery for missing reactions. | DISCORD; MODS; COMPLAINTS | Carl-bot and MEE6 reaction roles are common owner expectations. |
| BS-109 | Integrations | Autoroles | Autoroles can apply by invite source, screening result, tag, onboarding answer, time since join, or manual approval. | MODS; DISCORD; TELEGRAM | Bots and onboarding both apply roles automatically. |
| BS-110 | Integrations | Ticket workflow | Ticket channels are scoped, private, closeable, exportable, and role-routed without granting broad channel creation. | MODS; DISCORD | Ticket bots and role-exclusive channels make this a native need. |
| BS-111 | Integrations | Modmail | Member-to-moderator mail is a private beacon/ticket hybrid with reporter privacy and escalation roles. | MODS; DISCORD; TELEGRAM | Modmail bots and Telegram channel DMs inform inbound owner contact. |
| BS-112 | Integrations | Starboard | Optional starboard-style highlights are role/channel scoped and cannot leak hidden-channel content. | MODS; DISCORD | Carl-bot starboard patterns need privacy boundaries. |
| BS-113 | Integrations | Scheduled posts | Scheduled posts are native beacons with preview, role approval, cancellation, and failure alerts. | TELEGRAM; MODS; DISCORD | Telegram channel post workflows and bots support scheduling expectations. |
| BS-114 | Integrations | Custom commands | Custom commands are scoped scripts/templates with permission checks, rate limits, and no secret exfiltration path. | MODS; DISCORD; TELEGRAM | Discord bots and Telegram bots expose custom command demand. |
| BS-115 | Integrations | Client extensions | Local client extensions may change local presentation but cannot create access, decrypt content, or bypass policy. | MODS; DISCORD; TELEGRAM; COMPLAINTS | Vencord/BetterDiscord demand must be bounded by authority. |
| BS-116 | Integrations | Extension trust | Plugin-like extension install shows source, permissions, data access, update channel, and ability to disable safely. | MODS; COMPLAINTS | Third-party client trust concerns require explicit install posture. |
| BS-117 | Integrations | Translation | Translation is a per-member or per-channel assist feature that never changes canonical message content. | TELEGRAM; MODS | Telegram translation and client translation plugins show demand. |
| BS-118 | Integrations | Appearance | Themes, density, emoji rendering, role color visibility, and media previews are local preferences unless owner locks a compliance surface. | MODS; DISCORD; TELEGRAM | Client mods show customization demand without governance effects. |
| BS-119 | Integrations | Analytics | Owner analytics show aggregate channel health, onboarding completion, beacon delivery, and moderation backlog without member surveillance. | DISCORD; TELEGRAM; COMPLAINTS | Discord insights and Telegram statistics must not become private presence. |
| BS-120 | Integrations | Integration drift | Settings detect when bot, client, or integration permissions no longer match the Enclave policy and emit an admin beacon. | MODS; DISCORD; COMPLAINTS | Bot command sync and permission drift complaints require automatic detection. |
| BS-121 | Privacy and audit | Presence | Enclaves do not expose online, typing, read-receipt, or last-seen signals unless a later explicit feature gate permits it. | TELEGRAM; DISCORD; COMPLAINTS | Privacy posture must avoid importing platform presence expectations by accident. |
| BS-122 | Privacy and audit | Read receipts | Mandatory acknowledgement and read receipts are separate; acknowledgements are visible only to authorized roles. | DISCORD; TELEGRAM; COMPLAINTS | Rules acknowledgement should not create broad surveillance. |
| BS-123 | Privacy and audit | Screenshot claims | Owner settings must not claim screenshot prevention unless an implemented platform gate backs that exact claim. | DISCORD; TELEGRAM; COMPLAINTS | Feature confusion requires accurate capability language. |
| BS-124 | Privacy and audit | Content protection | Forwarding, saving, exporting, and screenshot policy are displayed as deterrence or client-control state, not as absolute privacy promises. | TELEGRAM; MODS; COMPLAINTS | Telegram content protection and client mods show limits of enforcement. |
| BS-125 | Privacy and audit | Data export | Owners can export governance, roles, resources, and beacon metadata; message plaintext export depends on member-held keys and policy. | DISCORD; TELEGRAM; MODS | Admin logs, client exports, and encrypted content require split export. |
| BS-126 | Privacy and audit | Admin log retention | Admin logs have configurable retention, tamper-evident hashes, and redaction markers for privacy-sensitive evidence. | TELEGRAM; DISCORD; MODS | Telegram recent actions and Discord audit logs need retention policy. |
| BS-127 | Privacy and audit | Permission diagnostics | A member or admin can ask why they can or cannot see a channel, command, role, or beacon, and receive a safe explanation. | DISCORD; MODS; COMPLAINTS | Permission viewer mods and recurring bot/role confusion justify diagnostics. |
| BS-128 | Privacy and audit | Failed save handling | Settings saves are transactional, name the failed field, preserve drafts, and provide retry rather than leaving partial state. | DISCORD; TELEGRAM; COMPLAINTS | Server Guide failed-save and Telegram unsaved-toggle complaints require this. |
| BS-129 | Privacy and audit | Undo window | Risky settings changes offer an undo window or rollback plan with preview of affected members and channels. | TELEGRAM; DISCORD; COMPLAINTS | Telegram undo patterns and Discord state complaints favor reversible changes. |
| BS-130 | Privacy and audit | Migration report | Migrating from another platform produces a report of unsupported settings, downgraded capabilities, and manual owner decisions. | DISCORD; TELEGRAM; MODS; COMPLAINTS | Discord/Telegram differences and bot/client extensions cannot import silently. |
| BS-131 | Privacy and audit | Claim allowlist | Owner-facing copy can claim only features represented in machine-readable settings and executable tests. | DISCORD; TELEGRAM; COMPLAINTS | Avoiding overclaims is necessary because features evolve across platforms. |
| BS-132 | Privacy and audit | Checker contract | The spec checker must fail any merged setting row that lacks DISCORD, TELEGRAM, MODS, or COMPLAINTS in its source marks. | DISCORD; TELEGRAM; MODS; COMPLAINTS | Task 4850 requires missing source marks to make the checker exit non-zero naming the row. |
