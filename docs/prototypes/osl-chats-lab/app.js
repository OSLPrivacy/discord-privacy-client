const STORAGE_KEY = "osl-chats-lab-v1";

const icons = {
  chats: `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M7 17.5 3.5 20v-5.2A8 8 0 0 1 3 12c0-4.4 4-8 9-8s9 3.6 9 8-4 8-9 8a10 10 0 0 1-5-1.5Z"/></svg>`,
  circles: `<svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="8" cy="9" r="3"/><circle cx="17" cy="8" r="2.5"/><path d="M2.8 19c.6-3.2 2.3-5 5.2-5s4.6 1.8 5.2 5M14 14c3.7-.7 6.2 1 7 4"/></svg>`,
  code: `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="m8 7-5 5 5 5M16 7l5 5-5 5M14 4l-4 16"/></svg>`,
  data: `<svg viewBox="0 0 24 24" aria-hidden="true"><ellipse cx="12" cy="5" rx="8" ry="3"/><path d="M4 5v6c0 1.7 3.6 3 8 3s8-1.3 8-3V5M4 11v6c0 1.7 3.6 3 8 3s8-1.3 8-3v-6"/></svg>`,
  settings: `<svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.9l.1.1-2.8 2.8-.1-.1a1.7 1.7 0 0 0-1.9-.3 1.7 1.7 0 0 0-1 1.6v.2h-4V21a1.7 1.7 0 0 0-1-1.6 1.7 1.7 0 0 0-1.9.3l-.1.1L4.2 17l.1-.1a1.7 1.7 0 0 0 .3-1.9A1.7 1.7 0 0 0 3 14H2.8v-4H3a1.7 1.7 0 0 0 1.6-1 1.7 1.7 0 0 0-.3-1.9L4.2 7 7 4.2l.1.1a1.7 1.7 0 0 0 1.9.3A1.7 1.7 0 0 0 10 3v-.2h4V3a1.7 1.7 0 0 0 1 1.6 1.7 1.7 0 0 0 1.9-.3l.1-.1L19.8 7l-.1.1a1.7 1.7 0 0 0-.3 1.9 1.7 1.7 0 0 0 1.6 1h.2v4H21a1.7 1.7 0 0 0-1.6 1Z"/></svg>`,
  search: `<svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="11" cy="11" r="7"/><path d="m16.3 16.3 4.2 4.2"/></svg>`,
  plus: `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 5v14M5 12h14"/></svg>`,
  lock: `<svg viewBox="0 0 24 24" aria-hidden="true"><rect x="5" y="10" width="14" height="11" rx="3"/><path d="M8 10V7a4 4 0 0 1 8 0v3"/></svg>`,
  phone: `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M8.2 3.7 10 7.8 7.8 9c1.2 3 3.4 5.2 6.3 6.4l1.3-2.3 4 1.8c.3.1.5.5.4.9-.5 2.5-2.4 4.2-4.8 4.2C9 19.7 4.3 15 4 9c-.1-2.4 1.6-4.3 4.2-4.8Z"/></svg>`,
  video: `<svg viewBox="0 0 24 24" aria-hidden="true"><rect x="3" y="6" width="13" height="12" rx="3"/><path d="m16 10 5-3v10l-5-3"/></svg>`,
  info: `<svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="9"/><path d="M12 11v6M12 7h.01"/></svg>`,
  back: `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="m15 18-6-6 6-6"/></svg>`,
  attach: `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="m20.5 11.5-8.2 8.2a5.4 5.4 0 0 1-7.7-7.7l8.3-8.3a3.7 3.7 0 1 1 5.2 5.2l-8.3 8.3a2 2 0 0 1-2.8-2.8l7.7-7.7"/></svg>`,
  image: `<svg viewBox="0 0 24 24" aria-hidden="true"><rect x="3" y="4" width="18" height="16" rx="3"/><circle cx="9" cy="9" r="2"/><path d="m4 17 5-5 4 4 2-2 5 4"/></svg>`,
  clock: `<svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/></svg>`,
  send: `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="m21 3-8 18-2.2-7.8L3 11l18-8Z"/><path d="m10.8 13.2 4.7-4.7"/></svg>`,
  download: `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 3v12M7 10l5 5 5-5M4 20h16"/></svg>`,
  palette: `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 3a9 9 0 1 0 0 18h1.2a1.8 1.8 0 0 0 1.3-3.1 1.8 1.8 0 0 1 1.3-3.1H18a3 3 0 0 0 3-3C21 7 17 3 12 3Z"/><circle cx="7.5" cy="11.5" r="1"/><circle cx="10" cy="7.5" r="1"/><circle cx="15" cy="7.5" r="1"/></svg>`,
  archive: `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 8h16v12H4zM3 4h18v4H3zM9 12h6"/></svg>`,
  device: `<svg viewBox="0 0 24 24" aria-hidden="true"><rect x="5" y="2" width="14" height="20" rx="3"/><path d="M10 18h4"/></svg>`,
  terminal: `<svg viewBox="0 0 24 24" aria-hidden="true"><rect x="3" y="4" width="18" height="16" rx="3"/><path d="m7 9 3 3-3 3M13 15h4"/></svg>`,
  users: `<svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="9" cy="8" r="3"/><circle cx="18" cy="9" r="2"/><path d="M3 20c.5-4 2.5-6 6-6s5.5 2 6 6M15 15c3-.7 5.3.8 6 4"/></svg>`,
};

const now = Date.now();
const minutesAgo = (minutes) => new Date(now - minutes * 60_000).toISOString();

const seed = {
  version: 1,
  route: "chats",
  activeConversation: "nova",
  activeChannel: "lobby",
  filter: "all",
  query: "",
  replyTo: null,
  mobileChatOpen: false,
  profile: {
    name: "Liam",
    username: "liam@osl",
    initials: "LW",
    avatarUrl: "",
    animated: true,
    status: "Building the private internet",
  },
  appearance: {
    theme: "midnight",
    accent: "#5fe5d2",
    density: "comfortable",
    fontScale: 1,
    messageShape: "rounded",
    motion: true,
    glass: true,
    profileFrame: "prism",
  },
  preferences: {
    enterToSend: true,
    readReceipts: true,
    typing: true,
    notificationPreview: false,
    linkPreviews: true,
    localSearch: true,
    directTransfer: true,
  },
  people: {
    nova: { id: "nova", name: "Nova Reyes", username: "nova@osl", initials: "NR", status: "online", verified: true, color: "linear-gradient(135deg,#fd7096,#8d6cff)", note: "Making an ambient game soundtrack", devices: 3 },
    theo: { id: "theo", name: "Theo Lin", username: "theo@osl", initials: "TL", status: "away", verified: true, color: "linear-gradient(135deg,#f0b557,#ea6f57)", note: "At the studio", devices: 2 },
    maya: { id: "maya", name: "Maya Okafor", username: "maya@osl", initials: "MO", status: "online", verified: true, color: "linear-gradient(135deg,#34c79b,#267dd6)", note: "Deep work until 3", devices: 4 },
    elio: { id: "elio", name: "Elio Park", username: "elio@osl", initials: "EP", status: "offline", verified: false, color: "linear-gradient(135deg,#78869b,#4c5c72)", note: "Offline", devices: 1 },
    forge: { id: "forge", name: "Forge Bot", username: "forge.bot", initials: "FB", status: "online", verified: true, color: "linear-gradient(135deg,#5ee4d1,#3276de)", note: "Local automation", devices: 1, bot: true },
  },
  conversations: [
    { id: "nova", type: "dm", name: "Nova Reyes", preview: "The relay receipt view is perfect.", time: "2m", unread: 2, pinned: true, personIds: ["nova"] },
    { id: "arcade", type: "circle", name: "Arcade Garden", preview: "Maya: pushed the lighting study", time: "8m", unread: 6, pinned: true, members: 38, color: "linear-gradient(135deg,#4655d8,#cb56d8)" },
    { id: "theo", type: "dm", name: "Theo Lin", preview: "Voice note · 0:42", time: "24m", unread: 0, pinned: false, personIds: ["theo"] },
    { id: "cipher", type: "circle", name: "Cipher Garden", preview: "Forge Bot: nightly archive complete", time: "1h", unread: 1, pinned: false, members: 112, color: "linear-gradient(135deg,#087f71,#31a6d8)" },
    { id: "maya", type: "dm", name: "Maya Okafor", preview: "Yes — one device per person makes sense.", time: "3h", unread: 0, pinned: false, personIds: ["maya"] },
    { id: "makers", type: "circle", name: "Midnight Makers", preview: "Theo: new build in #releases", time: "Yesterday", unread: 0, pinned: false, members: 16, color: "linear-gradient(135deg,#da5c7b,#d49d46)" },
    { id: "elio", type: "dm", name: "Elio Park", preview: "See you next week!", time: "Mon", unread: 0, pinned: false, personIds: ["elio"] },
    { id: "forge", type: "dm", name: "Forge Bot", preview: "3 local automations completed", time: "Mon", unread: 0, pinned: false, personIds: ["forge"] },
  ],
  circles: {
    arcade: { id: "arcade", name: "Arcade Garden", description: "Indie games, playful systems, and works in progress.", members: 38, online: 14, color: "linear-gradient(135deg,#4655d8,#cb56d8)", channels: [{id:"lobby",name:"lobby",unread:2},{id:"playtests",name:"playtests",unread:4},{id:"art-lab",name:"art-lab",unread:0},{id:"voice-lounge",name:"voice lounge",unread:0}] },
    cipher: { id: "cipher", name: "Cipher Garden", description: "Privacy engineering without the theater.", members: 112, online: 27, color: "linear-gradient(135deg,#087f71,#31a6d8)", channels: [{id:"lobby",name:"lobby",unread:1},{id:"protocol",name:"protocol",unread:0},{id:"builds",name:"builds",unread:0},{id:"research",name:"research",unread:0}] },
    makers: { id: "makers", name: "Midnight Makers", description: "A quiet room for people shipping strange, excellent things.", members: 16, online: 5, color: "linear-gradient(135deg,#da5c7b,#d49d46)", channels: [{id:"lobby",name:"lobby",unread:0},{id:"releases",name:"releases",unread:0},{id:"feedback",name:"feedback",unread:0}] },
  },
  messages: {
    nova: [
      { id:"n1", from:"nova", at:minutesAgo(74), text:"I tried the portable archive on my laptop. It opened instantly and the search index rebuilt locally.", reactions:[{e:"✦",n:2,mine:true}] },
      { id:"n2", from:"me", at:minutesAgo(70), text:"That is exactly the behavior I wanted. No account ceremony just to recover your own history." },
      { id:"n3", from:"nova", at:minutesAgo(42), text:"Also: make the relay deletion visible. People should be able to watch a file move from queued → delivered → gone from OSL." },
      { id:"n4", from:"nova", at:minutesAgo(40), text:"Not a vague promise. An actual receipt.", replyTo:"n3", reactions:[{e:"✓",n:1,mine:true},{e:"♡",n:1,mine:false}] },
      { id:"n5", from:"me", at:minutesAgo(15), text:"Added it to the storage model. Free is relay-and-drop; Pro is durable hosting. Encryption is identical." },
      { id:"n6", from:"nova", at:minutesAgo(2), text:"The relay receipt view is perfect.", attachment:{name:"relay-receipt-motion.fig",size:"4.8 MB",type:"FIG",state:"Local copy · relay deleted"} },
    ],
    theo: [
      { id:"t1", from:"theo", at:minutesAgo(180), text:"The new circle mix is almost done. Headphones recommended." },
      { id:"t2", from:"theo", at:minutesAgo(24), text:"Voice message", attachment:{name:"voice-note.opus",size:"0:42 · 384 KB",type:"AUDIO",state:"Delivered to 2 devices"} },
    ],
    maya: [
      { id:"m1", from:"me", at:minutesAgo(200), text:"Would you make relay completion per device or per person?" },
      { id:"m2", from:"maya", at:minutesAgo(180), text:"Per person by default. Otherwise a retired laptop makes storage permanent. Offer every-active-device as an explicit send option." },
      { id:"m3", from:"maya", at:minutesAgo(175), text:"Yes — one device per person makes sense." },
    ],
    elio: [{ id:"e1", from:"elio", at:minutesAgo(2500), text:"See you next week!" }],
    forge: [{ id:"f1", from:"forge", at:minutesAgo(3100), text:"3 local automations completed. No conversation content left this device." }],
    arcade: [
      { id:"a1", from:"maya", at:minutesAgo(63), text:"Pushed the lighting study. The reflections finally feel soft instead of plastic." },
      { id:"a2", from:"theo", at:minutesAgo(60), text:"This is gorgeous. Can we try it with the dusk palette too?", replyTo:"a1", reactions:[{e:"♡",n:4,mine:false}] },
      { id:"a3", from:"me", at:minutesAgo(18), text:"Started a thread for the dusk version so the main room stays readable." },
      { id:"a4", from:"forge", at:minutesAgo(8), text:"Thread created: Dusk lighting pass · 7 replies", systemBot:true },
    ],
    cipher: [
      { id:"c1", from:"forge", at:minutesAgo(70), text:"Nightly portable archive complete. 1,284 messages, 38 attachments, 0 relay blobs remaining." },
      { id:"c2", from:"maya", at:minutesAgo(60), text:"Can the audit file include device removals without exposing the device label to the server?" },
      { id:"c3", from:"me", at:minutesAgo(57), text:"Yes. The signed event can use the device public-key digest; friendly names stay local." },
    ],
    makers: [
      { id:"mk1", from:"theo", at:minutesAgo(1500), text:"New desktop build in #releases. Startup is under 300ms on my machine." },
      { id:"mk2", from:"nova", at:minutesAgo(1470), text:"Confirmed. The reduced-motion path feels much better too." },
    ],
  },
  bots: [
    { id:"forge-bot", name:"Forge", owner:"You", status:"running", events:12, accent:"#5fe5d2", permissions:["messages.read:selected", "messages.write:selected", "files.local", "circles.manage:selected"] },
    { id:"soundboard", name:"Soundboard", owner:"Arcade Garden", status:"paused", events:4, accent:"#a78bfa", permissions:["voice.play:selected", "commands.register"] },
  ],
  automations: [
    { id:"quiet-hours", name:"Quiet hours", trigger:"Weekdays at 10:30 PM", action:"Mute non-priority Circles until 7:30 AM", enabled:true, color:"#6be4cb" },
    { id:"save-designs", name:"Save design drops", trigger:"Message tagged #archive in Arcade Garden", action:"Copy attachments to ~/OSL/Arcade", enabled:true, color:"#a78bfa" },
    { id:"status-focus", name:"Focus status", trigger:"Calendar focus block begins", action:"Set local profile status and pause typing receipts", enabled:false, color:"#f6c76b" },
  ],
  events: [
    { at:"12:48:03", type:"message.created", detail:"dm:nova · local echo" },
    { at:"12:47:59", type:"blob.deleted", detail:"blb_83c1 · all recipients acknowledged" },
    { at:"12:47:58", type:"delivery.ack", detail:"nova/device_2 · attachment" },
    { at:"12:47:51", type:"automation.completed", detail:"save-designs · 1 local file" },
    { at:"12:46:22", type:"circle.thread.created", detail:"arcade/playtests · local event" },
    { at:"12:44:08", type:"device.verified", detail:"maya/key_17f9 · safety number matched" },
  ],
  storage: {
    plan:"free",
    localUsed:18.4,
    relayPending:1.2,
    hostedUsed:0,
    directTransfer:true,
    receiptTarget:"person",
    freeExpiry:"7 days",
  },
};

function cloneSeed() { return JSON.parse(JSON.stringify(seed)); }
function loadState() {
  try {
    const parsed = JSON.parse(localStorage.getItem(STORAGE_KEY));
    return parsed?.version === 1 ? parsed : cloneSeed();
  } catch { return cloneSeed(); }
}

let state = loadState();
const app = document.querySelector("#app");
const toastRegion = document.querySelector("#toast-region");
const avatarFile = document.querySelector("#avatar-file");

function save() { localStorage.setItem(STORAGE_KEY, JSON.stringify(state)); }
function escapeHtml(value = "") { return String(value).replace(/[&<>'"]/g, (char) => ({"&":"&amp;","<":"&lt;",">":"&gt;","'":"&#39;",'"':"&quot;"}[char])); }
function formatTime(iso) { return new Intl.DateTimeFormat(undefined,{hour:"numeric",minute:"2-digit"}).format(new Date(iso)); }
function initials(name) { return name.split(/\s+/).map((part)=>part[0]).join("").slice(0,2).toUpperCase(); }
function getConversation(id = state.activeConversation) { return state.conversations.find((item)=>item.id === id) ?? state.conversations[0]; }
function getPerson(id) { return state.people[id] ?? {id,name:"Unknown",username:"unknown",initials:"?",status:"offline",verified:false,color:"linear-gradient(135deg,#536176,#263347)",note:"",devices:0}; }
function addEvent(type, detail) {
  state.events.unshift({at:new Intl.DateTimeFormat(undefined,{hour:"2-digit",minute:"2-digit",second:"2-digit",hour12:false}).format(new Date()),type,detail});
  state.events = state.events.slice(0,40);
}
function toast(message, title = "OSL Chats") {
  const element = document.createElement("div");
  element.className = "toast";
  element.innerHTML = `<strong>${escapeHtml(title)}</strong><br>${escapeHtml(message)}`;
  toastRegion.append(element);
  setTimeout(()=>element.remove(),3800);
}

function avatarMarkup(person, className = "") {
  const image = person.avatarUrl ? `<img src="${escapeHtml(person.avatarUrl)}" alt="" />` : escapeHtml(person.initials ?? initials(person.name));
  return `<span class="avatar ${person.animated ? "animated" : ""} ${className}" data-initials="${escapeHtml(person.initials ?? initials(person.name))}" style="--avatar:${person.color ?? "linear-gradient(135deg,#5261d5,#55d6c2)"}">${image}${person.status === "online" ? '<i class="status-dot" aria-label="Online"></i>' : ""}</span>`;
}

function railMarkup() {
  const unread = state.conversations.reduce((sum,item)=>sum+item.unread,0);
  const items = [
    ["chats",icons.chats,"Chats",unread],
    ["circles",icons.circles,"Circles",0],
    ["developer",icons.code,"Develop",0],
    ["data",icons.data,"Data",0],
    ["settings",icons.settings,"Settings",0],
  ];
  return `<aside class="rail" aria-label="Primary navigation">
    <button class="brand-button" data-route="chats" aria-label="OSL Chats home"><svg class="brand-mark" viewBox="0 0 28 28"><path d="M6 9.5C7.7 6.8 10.4 5 14 5c5.2 0 9 3.7 9 8.6 0 4.7-3.6 8.4-9 8.4-1.9 0-3.6-.4-5-1.2L5 23l1.1-4.3A8 8 0 0 1 5 14.6"/><path d="M10 13.5h8M10 16.8h5"/></svg></button>
    <nav class="rail-nav">${items.map(([route,icon,label,badge])=>`<button class="rail-button ${state.route===route?"active":""}" data-route="${route}" aria-label="${label}" aria-current="${state.route===route?"page":"false"}">${icon}<span>${label}</span>${badge?`<b class="rail-badge">${badge}</b>`:""}</button>`).join("")}</nav>
    <div class="rail-spacer"></div>
    <button class="rail-button rail-profile" data-route="settings" aria-label="Open profile settings">${avatarMarkup({...state.profile,color:"linear-gradient(135deg,var(--accent),var(--violet))"},"mini-avatar")}</button>
  </aside>`;
}

function applyAppearance() {
  document.documentElement.dataset.theme = state.appearance.theme;
  document.documentElement.style.setProperty("--accent",state.appearance.accent);
  document.documentElement.style.setProperty("--accent-strong",state.appearance.accent);
  document.documentElement.style.setProperty("--density",state.appearance.density === "compact" ? ".78" : state.appearance.density === "airy" ? "1.16" : "1");
  document.documentElement.style.setProperty("--font-scale",String(state.appearance.fontScale));
  document.documentElement.style.setProperty("--message-radius",state.appearance.messageShape === "compact" ? "9px" : state.appearance.messageShape === "soft" ? "23px" : "18px");
}

function render() {
  applyAppearance();
  const content = state.route === "chats" ? renderChats() : state.route === "circles" ? renderCircles() : state.route === "developer" ? renderDeveloper() : state.route === "data" ? renderData() : renderSettings();
  app.innerHTML = `<div class="app-shell">${railMarkup()}${content}</div>`;
  bindEvents();
}

function conversationAvatar(conversation) {
  if (conversation.type === "circle") return avatarMarkup({name:conversation.name,initials:initials(conversation.name),color:conversation.color},"circle-avatar");
  return avatarMarkup(getPerson(conversation.personIds?.[0] ?? conversation.id));
}

function conversationListMarkup() {
  const query = state.query.trim().toLowerCase();
  const items = state.conversations.filter((item) => {
    const matchesType = state.filter === "all" || item.type === state.filter || (state.filter === "unread" && item.unread > 0);
    return matchesType && (!query || `${item.name} ${item.preview}`.toLowerCase().includes(query));
  });
  return `<aside class="conversation-list" aria-label="Conversations">
    <div class="list-header">
      <div class="title-row"><h1>Chats</h1><button class="icon-button" data-action="new-chat" aria-label="New chat">${icons.plus}</button></div>
      <div class="search-box">${icons.search}<input id="chat-search" value="${escapeHtml(state.query)}" placeholder="Search locally" aria-label="Search conversations" /></div>
      <div class="filter-tabs" role="tablist">${[["all","All"],["unread","Unread"],["dm","People"],["circle","Circles"]].map(([id,label])=>`<button class="filter-tab ${state.filter===id?"active":""}" data-filter="${id}" role="tab" aria-selected="${state.filter===id}">${label}</button>`).join("")}</div>
    </div>
    <div class="conversation-scroll"><div class="section-label"><span>${items.length} conversations</span><button data-action="mark-read">Read all</button></div>
      ${items.map((item)=>`<button class="conversation-item ${state.activeConversation===item.id?"active":""}" data-conversation="${item.id}">
        ${conversationAvatar(item)}<span class="conversation-copy"><span class="conversation-line"><span class="conversation-name">${escapeHtml(item.name)}</span>${item.type==="circle"?'<span class="badge local">Circle</span>':getPerson(item.id).verified?'<span class="verified" title="Keys verified">✓</span>':""}</span><span class="conversation-preview">${escapeHtml(item.preview)}</span></span>
        <span class="conversation-meta"><time>${escapeHtml(item.time)}</time>${item.unread?`<b class="unread-pill">${item.unread}</b>`:""}</span>
      </button>`).join("") || `<div class="empty">${icons.search}<h3>No local matches</h3><p>Your encrypted history is searched on this device.</p></div>`}
    </div>
  </aside>`;
}

function messageMarkup(message, allMessages) {
  const mine = message.from === "me";
  const person = mine ? {...state.profile,id:"me",color:"linear-gradient(135deg,var(--accent),var(--violet))"} : getPerson(message.from);
  const replied = message.replyTo ? allMessages.find((item)=>item.id===message.replyTo) : null;
  const timer = message.timer ? `<span class="message-rule">⏱ ${escapeHtml(message.timer)}</span>` : "";
  const viewOnce = message.viewOnce ? `<button class="view-once" data-action="open-once" data-message="${message.id}">◉ View once · ${escapeHtml(message.viewOnce)}</button>` : "";
  return `<article class="message-group ${mine?"mine":""}" data-message-id="${message.id}">
    ${mine?"":avatarMarkup(person,"message-avatar")}
    <div class="message-column">
      <div class="sender-line"><strong>${mine?"You":escapeHtml(person.name)}</strong><time datetime="${message.at}">${formatTime(message.at)}</time>${timer}</div>
      <div class="message-bubble">${replied?`<div class="reply-block"><strong>${replied.from==="me"?"You":escapeHtml(getPerson(replied.from).name)}</strong>${escapeHtml(replied.text).slice(0,95)}</div>`:""}${escapeHtml(message.text)}${viewOnce}</div>
      ${message.attachment?`<div class="attachment-card"><span class="file-icon">${escapeHtml(message.attachment.type)}</span><span><strong>${escapeHtml(message.attachment.name)}</strong><span>${escapeHtml(message.attachment.size)} · ${escapeHtml(message.attachment.state)}</span></span><button class="download-button" data-action="mock-download" aria-label="Save local copy">${icons.download}</button></div>`:""}
      ${message.reactions?.length?`<div class="reaction-row">${message.reactions.map((reaction)=>`<button class="reaction ${reaction.mine?"mine-reacted":""}" data-action="react" data-message="${message.id}" data-emoji="${reaction.e}">${reaction.e} ${reaction.n}</button>`).join("")}</div>`:""}
      <div class="message-actions"><button class="message-action" data-action="reply" data-message="${message.id}" aria-label="Reply">↩</button><button class="message-action" data-action="quick-react" data-message="${message.id}" aria-label="React">♡</button>${mine?`<button class="message-action" data-action="scrub-message" data-message="${message.id}" aria-label="Scrub my message">Scrub</button>`:""}<button class="message-action" data-action="message-rules" data-message="${message.id}" aria-label="Message rules">•••</button></div>
    </div>
  </article>`;
}

function chatDetailMarkup(conversation) {
  if (conversation.type === "circle") {
    const circle = state.circles[conversation.id];
    return `<aside class="detail-panel"><div class="detail-inner"><div class="detail-top">${conversationAvatar(conversation)}<h3>${escapeHtml(circle.name)}</h3><p>${circle.members} members · ${circle.online} online</p><div class="profile-actions"><button class="profile-action" data-action="call">${icons.phone}Voice</button><button class="profile-action" data-action="whiteboard">${icons.palette}Canvas</button><button class="profile-action" data-route="circles">${icons.settings}Manage</button></div></div>
      <section class="detail-section"><div class="security-card"><strong>${icons.lock} E2EE Circle</strong><p>Messages, calls, shared canvases and files are encrypted for approved Circle devices.</p></div></section>
      <section class="detail-section"><div class="detail-heading"><h4>Role powers</h4><button data-route="circles">Edit</button></div><div class="list"><div class="list-row"><span class="list-copy"><strong>Curators</strong><span>Threads, canvases, events</span></span><span class="badge">7</span></div><div class="list-row"><span class="list-copy"><strong>Stewards</strong><span>Clear shared history, roles, audit</span></span><span class="badge">3</span></div></div></section>
    </div></aside>`;
  }
  const person = getPerson(conversation.id);
  return `<aside class="detail-panel"><div class="detail-inner"><div class="detail-top">${avatarMarkup(person)}<h3>${escapeHtml(person.name)}</h3><p>${escapeHtml(person.username)} · ${person.devices} devices</p><div class="profile-actions"><button class="profile-action" data-action="call">${icons.phone}Call</button><button class="profile-action" data-action="whiteboard">${icons.palette}Canvas</button><button class="profile-action" data-action="friend-card">${icons.users}Friend</button></div></div>
    <section class="detail-section"><div class="security-card"><strong>${icons.lock} ${person.verified?"Verified connection":"Verify keys"}</strong><p>Safety number 72 18 44 90 · messages and calls use end-to-end encryption.</p></div></section>
    <section class="detail-section"><div class="detail-heading"><h4>Shared studio</h4><button data-action="whiteboard">Open</button></div><div class="media-grid"><div class="media-tile" style="--tile:linear-gradient(135deg,#392d62,#dc608d)"></div><div class="media-tile" style="--tile:linear-gradient(135deg,#164f52,#65dfb7)"></div><div class="media-tile" style="--tile:linear-gradient(135deg,#6c412c,#e7b55f)"></div></div></section>
    <section class="detail-section"><div class="setting-row"><span>Mute locally</span><button class="toggle" data-action="toggle-generic" aria-label="Mute locally"></button></div><div class="setting-row"><span>Hide previews</span><button class="toggle on" data-action="toggle-generic" aria-label="Hide previews"></button></div></section>
  </div></aside>`;
}

function renderChats() {
  const conversation = getConversation();
  const circle = conversation.type === "circle" ? state.circles[conversation.id] : null;
  const person = conversation.type === "dm" ? getPerson(conversation.id) : null;
  const messages = state.messages[conversation.id] ?? [];
  const channels = circle ? `<div class="channel-bar" aria-label="Circle channels">${circle.channels.map((channel)=>`<button class="channel-tab ${state.activeChannel===channel.id?"active":""}" data-channel="${channel.id}"># ${escapeHtml(channel.name)}${channel.unread?` · ${channel.unread}`:""}</button>`).join("")}<button class="channel-tab" data-action="whiteboard">✦ live canvas</button><button class="channel-tab" data-action="call">◉ voice lounge</button></div>` : "";
  return `<main id="main-content" class="workspace chat-layout ${state.mobileChatOpen?"mobile-chat-open":""}">${conversationListMarkup()}
    <section class="chat-panel" aria-label="${escapeHtml(conversation.name)} chat">
      <div><header class="chat-header"><div class="chat-person"><button class="icon-button mobile-back" data-action="mobile-back" aria-label="Back">${icons.back}</button>${conversationAvatar(conversation)}<div><h2>${escapeHtml(conversation.name)}</h2><div class="presence">${circle?`${circle.online} online · ${circle.members} members`:`<strong>${person.status}</strong> · ${escapeHtml(person.note)}`}</div></div></div><div class="header-actions"><button class="icon-button optional-action" data-action="call" aria-label="Voice call">${icons.phone}</button><button class="icon-button optional-action" data-action="video-call" aria-label="Video call">${icons.video}</button><button class="icon-button security-button" data-action="security" aria-label="Encryption details">${icons.lock}<span>E2EE</span></button></div></header>${channels}</div>
      <div class="message-scroll" id="message-scroll"><div class="encryption-notice">${icons.lock} End-to-end encrypted · history lives on your devices</div><div class="day-divider">Today</div>${messages.map((message)=>messageMarkup(message,messages)).join("")}</div>
      <div class="composer-wrap">${state.replyTo?`<div class="replying-banner visible"><span>Replying to ${escapeHtml(state.replyTo)}</span><button data-action="cancel-reply">Cancel</button></div>`:""}<form class="composer" id="message-form"><div class="composer-tools"><button type="button" class="composer-button" data-action="attach" aria-label="Attach file">${icons.attach}</button><button type="button" class="composer-button secondary-tool" data-action="view-once" aria-label="View-once media">${icons.image}</button><button type="button" class="composer-button secondary-tool" data-action="timer" aria-label="Message timer">${icons.clock}</button><button type="button" class="composer-button secondary-tool" data-action="whiteboard" aria-label="Share canvas">${icons.palette}</button></div><textarea id="message-input" placeholder="Message ${escapeHtml(conversation.name)}" aria-label="Message"></textarea><button class="send-button" aria-label="Send">${icons.send}</button></form><div class="composer-footer"><span>Local draft · Markdown, GIFs, polls, code and voice</span><span><kbd>Enter</kbd> send · <kbd>Shift</kbd>+<kbd>Enter</kbd> line</span></div></div>
    </section>${chatDetailMarkup(conversation)}
  </main>`;
}

function renderCircles() {
  return `<main id="main-content" class="workspace content-layout"><div class="page"><div class="page-shell"><header class="page-header"><div><span class="eyebrow">Communities without compromise</span><h1>Circles</h1><p>Encrypted spaces with channels, threads, calls, stages, forums, live canvases, events, bots, custom profiles and role systems that can be as simple or precise as you want.</p></div><button class="primary-button" data-action="new-circle">Create Circle</button></header>
  <div class="grid three">${Object.values(state.circles).map((circle)=>`<article class="card circle-card"><div class="circle-hero" style="--cover:${circle.color}"><div class="circle-hero-content"><h2>${escapeHtml(circle.name)}</h2><p>${circle.members} members · ${circle.online} online · E2EE</p></div></div><div class="circle-body"><p>${escapeHtml(circle.description)}</p><div class="circle-stats"><span>◉ Voice + stage</span><span>✦ Canvases</span><span>⌘ Bots</span></div><div class="channel-list">${circle.channels.slice(0,3).map((channel)=>`<div class="channel-row"><span># ${escapeHtml(channel.name)}</span><span>${channel.unread||""}</span></div>`).join("")}</div><button class="secondary-button full-width" data-open-circle="${circle.id}">Open Circle</button></div></article>`).join("")}</div>
  <section class="section-block"><div class="card"><div class="card-head"><div><span class="eyebrow">Permission studio</span><h2>Roles are composable, scoped and reviewable</h2></div><span class="badge local">Local policy preview</span></div><div class="capability-matrix">${[
    ["Own content sovereignty","Every member may instantly scrub all of their own messages and files."],["Shared history","Only roles explicitly granted Clear shared history can remove other members’ content."],["Canvas roles","Separate draw, edit objects, present, export, and clear powers."],["Voice roles","Per-room speak, stream, record-consent, soundboard, stage and moderation powers."],["Conditional roles","Time-boxed, channel-scoped, quorum-approved and bot-assumable roles."],["Audit without plaintext","Signed policy changes and moderator actions; content stays encrypted."]
  ].map(([title,copy])=>`<div class="capability"><strong>${title}</strong><span>${copy}</span></div>`).join("")}</div></div></section>
  </div></div></main>`;
}

function renderDeveloper() {
  const permissions = ["messages.read:selected","messages.write:selected","files.local","voice.play:selected","canvas.edit:selected","circles.manage:selected","contacts.read:selected","presence.write","notifications.local","exports.create"];
  return `<main id="main-content" class="workspace content-layout"><div class="page"><div class="page-shell"><header class="page-header"><div><span class="eyebrow">Official extension platform</span><h1>Build on OSL</h1><p>Bots, user automations, themes and embedded tools are first-class. Everything runs locally by default, with capability grants you can inspect and revoke.</p></div><button class="primary-button" data-action="create-bot">New project</button></header>
  <div class="grid two"><section class="card"><div class="card-head"><div><h2>Your projects</h2><p>Bots and self-automations use the same documented SDK.</p></div><span class="badge live">Dev runtime live</span></div><div class="list">${state.bots.map((bot)=>`<div class="list-row"><span class="bot-dot" style="--bot:${bot.accent}"></span><span class="list-copy"><strong>${escapeHtml(bot.name)}</strong><span>${escapeHtml(bot.owner)} · ${bot.events} events · ${bot.permissions.length} grants</span></span><button class="toggle ${bot.status==="running"?"on":""}" data-bot-toggle="${bot.id}" aria-label="Toggle ${escapeHtml(bot.name)}"></button></div>`).join("")}</div></section>
  <section class="card"><div class="card-head"><div><h2>Live event stream</h2><p>Payload previews are generated on this device.</p></div><button class="secondary-button" data-action="emit-event">Emit test</button></div><div class="event-stream">${state.events.map((event)=>`<div class="event"><time>${event.at}</time><span><strong>${escapeHtml(event.type)}</strong><br>${escapeHtml(event.detail)}</span></div>`).join("")}</div></section>
  <section class="card"><div class="card-head"><div><h2>Capability grants</h2><p>No ambient access. People approve the exact rooms, events, files and durations.</p></div><span class="badge local">User controlled</span></div><div class="permission-grid">${permissions.map((permission,index)=>`<label class="permission"><input type="checkbox" ${index<5?"checked":""} data-permission="${permission}" />${permission}</label>`).join("")}</div></section>
  <section class="card"><div class="card-head"><div><h2>Local SDK</h2><p>Typed events, commands, panels, canvases, call tools and local data APIs.</p></div><button class="secondary-button" data-action="copy-code">Copy</button></div><pre class="code" id="sdk-code">import { defineProject } from "@osl/chat-sdk";

export default defineProject({
  permissions: ["messages.write:selected"],
  onMessage({ message, reply }) {
    if (message.text === "/ship") reply("Build queued locally ✓");
  }
});</pre></section></div>
  <section class="section-block"><div class="card"><div class="card-head"><div><span class="eyebrow">Personal automations</span><h2>Your account, programmable by you</h2></div><button class="secondary-button" data-action="new-automation">Add automation</button></div><div class="grid three">${state.automations.map((item)=>`<article class="card automation-card" style="--automation-accent:${item.color}"><div class="card-head"><h3>${escapeHtml(item.name)}</h3><button class="toggle ${item.enabled?"on":""}" data-automation-toggle="${item.id}" aria-label="Toggle ${escapeHtml(item.name)}"></button></div><p><strong>When:</strong> ${escapeHtml(item.trigger)}<br><strong>Then:</strong> ${escapeHtml(item.action)}</p><span class="badge local">Runs locally</span></article>`).join("")}</div></div></section>
  </div></div></main>`;
}

function renderData() {
  return `<main id="main-content" class="workspace content-layout"><div class="page"><div class="page-shell"><header class="page-header"><div><span class="eyebrow">Portable by design</span><h1>Your data, instantly</h1><p>Download usable data at any time. Free keeps full history on your devices or storage; Pro only pays for durable OSL-operated encrypted hosting.</p></div><button class="primary-button" data-export="archive">Download everything</button></header>
  <div class="grid three"><div class="card"><span class="badge local">On this device</span><div class="metric">${state.storage.localUsed} GB</div><div class="metric-label">Messages, media, search and canvases</div></div><div class="card"><span class="badge">Temporary relay</span><div class="metric">${state.storage.relayPending} GB</div><div class="metric-label">Encrypted blobs awaiting receipt</div></div><div class="card"><span class="badge ${state.storage.plan==="pro"?"live":""}">${state.storage.plan}</span><div class="metric">${state.storage.hostedUsed} GB</div><div class="metric-label">Durable OSL-hosted storage</div></div></div>
  <section class="section-block grid two"><article class="card"><div class="card-head"><div><h2>Free · own your storage</h2><p>The complete messenger. No security, calling, customization, bot, Circle or export feature is paywalled.</p></div><span class="badge local">Current</span></div><div class="feature-list">${["Unlimited local history","Direct and LAN transfers","Connect your own S3/WebDAV/NAS/cloud drive","Encrypted relay-and-delete for offline recipients","Full E2EE calls, Circles, canvases and bots","Instant JSON, CSV, HTML and portable exports"].map(item=>`<span>✓ ${item}</span>`).join("")}</div></article><article class="card pro-card"><div class="card-head"><div><h2>Pro · OSL hosts it for you</h2><p>Convenience and infrastructure—not better privacy or a better messenger.</p></div><span class="badge">Optional</span></div><div class="feature-list">${["Durable encrypted large-file hosting","Multi-device restoration without an online peer","Redundant copies and version history","Long retention and background availability","Priority relay for faster global delivery"].map(item=>`<span>＋ ${item}</span>`).join("")}</div><button class="secondary-button" data-action="toggle-plan">Preview Pro storage</button></article></section>
  <section class="section-block grid two"><article class="card"><div class="card-head"><div><h2>Relay receipt</h2><p>The service deletes ciphertext when the selected completion rule is met, or when the expiry wins.</p></div><span class="badge live">Verifiable</span></div><div class="relay-track"><div class="relay-step done"><b>1</b><span>Encrypted locally</span></div><div class="relay-step done"><b>2</b><span>Recipient acknowledged</span></div><div class="relay-step done"><b>3</b><span>Relay blob deleted</span></div></div><div class="form-grid"><label class="field"><span>Complete after</span><select id="receipt-target"><option value="person" ${state.storage.receiptTarget==="person"?"selected":""}>One device per recipient</option><option value="active" ${state.storage.receiptTarget==="active"?"selected":""}>Every active device</option></select></label><label class="field"><span>Maximum relay life</span><select id="relay-expiry"><option>24 hours</option><option ${state.storage.freeExpiry==="7 days"?"selected":""}>7 days</option><option>30 days</option></select></label></div></article>
  <article class="card"><div class="card-head"><div><h2>Instant downloads</h2><p>Readable formats plus a signed portable archive for importing into another OSL-compatible client.</p></div>${icons.download}</div><div class="list"><button class="list-row export-row" data-export="json"><span class="list-copy"><strong>Messages + settings · JSON</strong><span>Structured and developer friendly</span></span><span>Download</span></button><button class="list-row export-row" data-export="csv"><span class="list-copy"><strong>Messages · CSV</strong><span>Spreadsheet compatible</span></span><span>Download</span></button><button class="list-row export-row" data-export="html"><span class="list-copy"><strong>Readable archive · HTML</strong><span>Opens without OSL</span></span><span>Download</span></button><button class="list-row export-row" data-export="archive"><span class="list-copy"><strong>Portable archive · .oslarchive</strong><span>Signed manifest + local data snapshot</span></span><span>Download</span></button></div></article></section>
  </div></div></main>`;
}

function renderSettings() {
  const themes = [["midnight","Midnight","linear-gradient(135deg,#080b11,#273852)"],["aurora","Aurora","linear-gradient(135deg,#071814,#16433c)"],["orchid","Orchid","linear-gradient(135deg,#160d22,#633f72)"],["paper","Paper","linear-gradient(135deg,#dce6e7,#697b82)"]];
  const toggles = [["readReceipts","Read receipts"],["typing","Typing indicators"],["notificationPreview","Notification previews"],["linkPreviews","Private link previews"],["localSearch","Local full-text search"],["directTransfer","Prefer direct transfer"]];
  return `<main id="main-content" class="workspace content-layout"><div class="page"><div class="page-shell"><header class="page-header"><div><span class="eyebrow">Your client, your canvas</span><h1>Make it unmistakably yours</h1><p>Animated identities, per-Circle profiles, deep themes, custom CSS, sounds, layouts and interaction rules are included for everyone.</p></div><button class="secondary-button" data-action="reset">Reset lab</button></header>
  <div class="grid two"><section class="card"><div class="card-head"><div><h2>Profile studio</h2><p>GIF, APNG, WebP or still images. Animation and frames are free.</p></div><div class="frame-preview">${avatarMarkup({...state.profile,color:"linear-gradient(135deg,var(--accent),var(--violet))"})}</div></div><div class="form-grid"><label class="field"><span>Display name</span><input id="profile-name" value="${escapeHtml(state.profile.name)}" /></label><label class="field"><span>Universal handle</span><input id="profile-handle" value="${escapeHtml(state.profile.username)}" /></label><label class="field full"><span>Status / profile line</span><input id="profile-status" value="${escapeHtml(state.profile.status)}" /></label><label class="field full"><span>Avatar image or GIF URL</span><input id="avatar-url" value="${escapeHtml(state.profile.avatarUrl)}" placeholder="https://…/avatar.gif" /><small>Or choose a local image. It stays in this browser prototype.</small></label></div><div class="button-row"><button class="secondary-button" data-action="choose-avatar">Choose image / GIF</button><button class="secondary-button" data-action="save-profile">Save profile</button></div></section>
  <section class="card"><div class="card-head"><div><h2>Theme studio</h2><p>Start with a theme, then change every token or add scoped CSS.</p></div>${icons.palette}</div><div class="theme-grid">${themes.map(([id,label,preview])=>`<button class="theme-card ${state.appearance.theme===id?"active":""}" data-theme-choice="${id}" style="--preview:${preview}"><span>${label}</span></button>`).join("")}</div><p class="field-label">Accent</p><div class="color-row">${["#5fe5d2","#8da2ff","#e979b5","#f4bd62","#f07b72","#9be062"].map(color=>`<button class="color-swatch ${state.appearance.accent===color?"active":""}" data-accent="${color}" style="--swatch:${color}" aria-label="Use ${color}"></button>`).join("")}</div><div class="form-grid settings-gap"><label class="field"><span>Density</span><select id="density"><option value="compact" ${state.appearance.density==="compact"?"selected":""}>Compact</option><option value="comfortable" ${state.appearance.density==="comfortable"?"selected":""}>Comfortable</option><option value="airy" ${state.appearance.density==="airy"?"selected":""}>Airy</option></select></label><label class="field"><span>Message shape</span><select id="message-shape"><option value="compact">Precise</option><option value="rounded" ${state.appearance.messageShape==="rounded"?"selected":""}>Rounded</option><option value="soft" ${state.appearance.messageShape==="soft"?"selected":""}>Soft</option></select></label><label class="field full"><span>Font scale · ${Math.round(state.appearance.fontScale*100)}%</span><input id="font-scale" type="range" min="0.85" max="1.25" step="0.05" value="${state.appearance.fontScale}" /></label></div></section>
  <section class="card"><div class="card-head"><div><h2>Conversation behavior</h2><p>Receipts and presence are mutual-consent preferences in real E2EE conversations.</p></div></div>${toggles.map(([key,label])=>`<div class="setting-row"><span>${label}</span><button class="toggle ${state.preferences[key]?"on":""}" data-pref="${key}" aria-label="Toggle ${label}"></button></div>`).join("")}</section>
  <section class="card"><div class="card-head"><div><h2>Creator mode</h2><p>Personalize more than color: per-space identities, sound packs, typography, layouts, motion, stickers and CSS.</p></div><span class="badge local">All free</span></div><div class="capability-matrix compact">${["Per-Circle avatar + bio","Animated avatar frames","Custom emoji + sticker packs","Per-contact chat themes","Custom notification sounds","Layout + sidebar composer","CSS token editor + inspector","Import/export theme bundles"].map(item=>`<div class="capability"><strong>${item}</strong><span>Ready for the full product spec</span></div>`).join("")}</div><button class="secondary-button" data-action="custom-css">Open CSS studio</button></section>
  <section class="card full"><div class="card-head"><div><span class="eyebrow">Power deck</span><h2>The useful parts of mods become supported features</h2><p>Discoverable, documented and permissioned—without depending on fragile client patches.</p></div><span class="badge local">Modular</span></div><div class="capability-matrix">${[
    ["Conversation cockpit","Tabbed chats, folders, favorites, pins, drafts, jump history and full search context."],["Media laboratory","GIF favorites and search, image zoom, original quality, filename privacy, media editor and watch rooms."],["Message power tools","Send later, send silently, quick edit, custom commands, translation, URL cleanup and expanded reactions."],["Presence + identity","Per-Circle profiles, rich presence, custom idle, streamer mode, account switching and session controls."],["Accessible by construction","Font control, color-safe roles, reduced motion, alt-text helpers, captions and keyboard command palette."],["Local sovereignty","Personal filters, keyword rules, client-side block, notification routing, local archive and portable settings sync."]
  ].map(([title,copy])=>`<div class="capability"><strong>${title}</strong><span>${copy}</span></div>`).join("")}</div></section></div>
  </div></div></main>`;
}

function showModal(title, body, actions = "") {
  document.querySelector(".modal-layer")?.remove();
  const layer = document.createElement("div");
  layer.className = "modal-layer";
  layer.innerHTML = `<section class="modal" role="dialog" aria-modal="true" aria-labelledby="modal-title"><header class="modal-header"><div><span class="eyebrow">OSL Chats Lab</span><h2 id="modal-title">${escapeHtml(title)}</h2></div><button class="modal-close" data-action="close-modal" aria-label="Close">×</button></header>${body}${actions?`<footer class="modal-actions">${actions}</footer>`:""}</section>`;
  document.body.append(layer);
  layer.querySelector("button, input, select, textarea")?.focus();
}

function closeModal() { document.querySelector(".modal-layer")?.remove(); }

function messageRulesModal(messageId = "new") {
  showModal("Message privacy rules",`<p class="modal-lead">Rules are encrypted into the message envelope. Recipients cannot use an OSL client to bypass view-once or expiry.</p><div class="form-grid">
    <label class="field"><span>Message lifetime</span><select id="rule-timer"><option value="off">Keep until I scrub it</option><option value="30 seconds">30 seconds after viewing</option><option value="5 minutes">5 minutes after viewing</option><option value="1 hour">1 hour after viewing</option><option value="1 day">1 day after viewing</option><option value="custom">Custom…</option></select></label>
    <label class="field"><span>Open allowance</span><select id="rule-opens"><option>Unlimited</option><option>View once</option><option>2 opens</option><option>5 opens</option></select></label>
    <label class="field"><span>Timer begins</span><select><option>When each person opens it</option><option>When everyone receives it</option><option>At a specific time</option></select></label>
    <label class="field"><span>Forwarding</span><select><option>Disallow in OSL</option><option>Ask me</option><option>Allow with attribution</option></select></label>
    <label class="field full"><span>Custom duration</span><input placeholder="e.g. 12 minutes, 3 days, or Aug 4 at 9 PM" /></label>
  </div><div class="security-card modal-note"><strong>${icons.lock} Honest boundary</strong><p>OSL can block screenshots in supported environments and enforce rules in trusted clients, but no messenger can stop an external camera or a modified recipient device.</p></div>`,`<button class="secondary-button" data-action="close-modal">Cancel</button><button class="primary-button" data-apply-rules="${messageId}">Apply rule</button>`);
}

function whiteboardModal() {
  showModal("Live encrypted canvas",`<div class="canvas-toolbar"><button class="tool-chip active" data-canvas-tool="pen">Pen</button><button class="tool-chip" data-canvas-tool="note">Note</button><button class="tool-chip" data-canvas-tool="frame">Frame</button><input id="canvas-color" type="color" value="${state.appearance.accent}" aria-label="Drawing color" /><button class="tool-chip" data-action="clear-canvas">Clear mine</button></div><canvas id="whiteboard" width="900" height="440" aria-label="Collaborative whiteboard"></canvas><div class="canvas-presence"><span><i style="--presence:#5fe5d2"></i>You</span><span><i style="--presence:#e979b5"></i>Nova</span><span><i style="--presence:#f4bd62"></i>Maya</span><span class="badge local">E2EE · local draft</span></div>`,`<button class="secondary-button" data-action="close-modal">Keep private</button><button class="primary-button" data-action="share-canvas">Display in chat</button>`);
  bindCanvas();
}

function bindCanvas() {
  const canvas = document.querySelector("#whiteboard");
  if (!canvas) return;
  const context = canvas.getContext("2d");
  const fit = () => { canvas.style.aspectRatio = `${canvas.width}/${canvas.height}`; };
  fit();
  context.lineCap = "round";
  context.lineJoin = "round";
  context.lineWidth = 4;
  context.strokeStyle = state.appearance.accent;
  let drawing = false;
  const point = (event) => { const rect=canvas.getBoundingClientRect(); const input=event.touches?.[0]??event; return {x:(input.clientX-rect.left)*canvas.width/rect.width,y:(input.clientY-rect.top)*canvas.height/rect.height}; };
  const start = (event) => { drawing=true; const p=point(event); context.beginPath(); context.moveTo(p.x,p.y); };
  const move = (event) => { if(!drawing)return; event.preventDefault(); const p=point(event); context.strokeStyle=document.querySelector("#canvas-color")?.value??state.appearance.accent; context.lineTo(p.x,p.y); context.stroke(); };
  const end = () => { if(drawing) addEvent("canvas.stroke.created","local draft · encrypted on share"); drawing=false; };
  canvas.addEventListener("pointerdown",start); canvas.addEventListener("pointermove",move); window.addEventListener("pointerup",end,{once:true});
}

function callModal(video = false) {
  const conversation = getConversation();
  showModal(`${video?"Video":"Voice"} room · ${conversation.name}`,`<div class="call-stage"><div class="call-orbit">${conversationAvatar(conversation)}<span class="call-wave"></span></div><strong>Ready to start</strong><p>Spatial audio, screen sharing, collaborative canvas, soundboard, captions and recording-by-consent are available in DMs, groups and Circle rooms.</p><div class="call-tools"><button class="profile-action">Mic</button><button class="profile-action">Camera</button><button class="profile-action">Screen</button><button class="profile-action" data-action="whiteboard">Canvas</button><button class="profile-action">Sounds</button></div><div class="security-card"><strong>${icons.lock} End-to-end encrypted call</strong><p>Room keys are held by current participants. Recording requires a visible consent grant from every participant.</p></div></div>`,`<button class="secondary-button" data-action="close-modal">Cancel</button><button class="primary-button" data-action="start-call">Start encrypted call</button>`);
}

function friendModal() {
  showModal("Add anyone, without friction",`<div class="grid two"><button class="friend-method"><b>@</b><strong>Universal handle</strong><span>Type a name like nova@osl</span></button><button class="friend-method"><b>⌁</b><strong>Scan nearby</strong><span>Bluetooth / LAN, no server lookup</span></button><button class="friend-method"><b>▦</b><strong>QR contact card</strong><span>Keys and profile in one scan</span></button><button class="friend-method"><b>↗</b><strong>Private invite link</strong><span>Single-use or expiring</span></button><button class="friend-method"><b>◇</b><strong>Contact discovery</strong><span>Private matching, optional</span></button><button class="friend-method"><b>✦</b><strong>Friend bundle</strong><span>Share a curated contact group</span></button></div><label class="field modal-note"><span>Handle, invite, phone, email or public-key fingerprint</span><input id="friend-query" placeholder="nova@osl" /></label>`,`<button class="secondary-button" data-action="close-modal">Cancel</button><button class="primary-button" data-action="send-friend-request">Send encrypted request</button>`);
}

function circleModal() {
  showModal("Create an encrypted Circle",`<div class="form-grid"><label class="field full"><span>Name</span><input id="circle-name" placeholder="Your new space" /></label><label class="field"><span>Shape</span><select><option>Community</option><option>Private group</option><option>Studio</option><option>Team</option><option>Broadcast + discussion</option></select></label><label class="field"><span>Joining</span><select><option>Invite only</option><option>Approval required</option><option>Public directory</option></select></label><label class="field full"><span>Start with</span><div class="permission-grid"><label class="permission"><input type="checkbox" checked />Chat channels</label><label class="permission"><input type="checkbox" checked />Voice lounge</label><label class="permission"><input type="checkbox" checked />Live canvas</label><label class="permission"><input type="checkbox" />Forum</label><label class="permission"><input type="checkbox" />Stage</label><label class="permission"><input type="checkbox" />Project board</label></div></label></div>`,`<button class="secondary-button" data-action="close-modal">Cancel</button><button class="primary-button" data-action="create-circle-confirm">Create locally</button>`);
}

function projectModal() {
  showModal("New developer project",`<div class="form-grid"><label class="field full"><span>Project name</span><input id="project-name" placeholder="My local companion" /></label><label class="field"><span>Runs as</span><select><option>Bot identity</option><option>User automation</option><option>Embedded chat tool</option><option>Theme + UI extension</option></select></label><label class="field"><span>Runtime</span><select><option>Local sandbox</option><option>My server</option><option>WebAssembly</option></select></label><label class="field full"><span>Starter</span><select><option>Command bot</option><option>Message workflow</option><option>Collaborative canvas tool</option><option>Call activity</option><option>Blank TypeScript project</option></select></label></div><div class="security-card modal-note"><strong>Capability-based by default</strong><p>The project receives nothing until the user grants a narrow permission for selected people, Circles or channels.</p></div>`,`<button class="secondary-button" data-action="close-modal">Cancel</button><button class="primary-button" data-action="create-project-confirm">Create project</button>`);
}

function downloadBlob(name, type, content) {
  const url = URL.createObjectURL(new Blob([content],{type}));
  const anchor = Object.assign(document.createElement("a"),{href:url,download:name});
  document.body.append(anchor); anchor.click(); anchor.remove(); setTimeout(()=>URL.revokeObjectURL(url),1000);
}

function exportData(format) {
  const snapshot = {exportedAt:new Date().toISOString(),formatVersion:1,profile:state.profile,people:state.people,conversations:state.conversations,messages:state.messages,circles:state.circles,settings:{appearance:state.appearance,preferences:state.preferences},manifest:{encryptedAtRest:false,prototype:true,signature:"LOCAL-PROTOTYPE-NOT-SIGNED"}};
  if (format === "csv") {
    const rows = [["conversation","message_id","sender","timestamp","text"],...Object.entries(state.messages).flatMap(([conversation,messages])=>messages.map((message)=>[conversation,message.id,message.from,message.at,message.text]))];
    downloadBlob("osl-messages.csv","text/csv",rows.map(row=>row.map(value=>`"${String(value).replaceAll('"','""')}"`).join(",")).join("\n"));
  } else if (format === "html") {
    downloadBlob("osl-readable-archive.html","text/html",`<!doctype html><meta charset="utf-8"><title>OSL archive</title><style>body{font:16px system-ui;max-width:760px;margin:40px auto;padding:0 20px}article{padding:12px 0;border-bottom:1px solid #ddd}small{color:#667}</style><h1>OSL Chats archive</h1>${Object.entries(state.messages).map(([id,messages])=>`<h2>${escapeHtml(getConversation(id)?.name??id)}</h2>${messages.map(message=>`<article><b>${escapeHtml(message.from==="me"?state.profile.name:getPerson(message.from).name)}</b> <small>${escapeHtml(message.at)}</small><p>${escapeHtml(message.text)}</p></article>`).join("")}`).join("")}`);
  } else {
    downloadBlob(format === "archive" ? "osl-portable.oslarchive" : "osl-data.json","application/json",JSON.stringify(snapshot,null,2));
  }
  addEvent("export.created",`${format} · local download`); toast(`${format.toUpperCase()} export created locally`,"Download ready");
}

function sendMessage() {
  const input = document.querySelector("#message-input");
  const text = input?.value.trim();
  if (!text) return;
  const conversation = getConversation();
  const message = {id:`local-${Date.now()}`,from:"me",at:new Date().toISOString(),text,replyTo:state.replyTo};
  (state.messages[conversation.id] ??= []).push(message);
  conversation.preview = `You: ${text}`; conversation.time = "now"; state.replyTo = null;
  addEvent("message.created",`${conversation.type}:${conversation.id} · local echo`); save(); render();
  requestAnimationFrame(()=>{ const scroll=document.querySelector("#message-scroll"); if(scroll)scroll.scrollTop=scroll.scrollHeight; });
}

function bindEvents() {
  app.querySelectorAll("[data-route]").forEach((button)=>button.addEventListener("click",()=>{state.route=button.dataset.route;state.mobileChatOpen=false;save();render();}));
  app.querySelectorAll("[data-conversation]").forEach((button)=>button.addEventListener("click",()=>{state.activeConversation=button.dataset.conversation;state.activeChannel="lobby";state.mobileChatOpen=true;const item=getConversation();item.unread=0;save();render();}));
  app.querySelectorAll("[data-filter]").forEach((button)=>button.addEventListener("click",()=>{state.filter=button.dataset.filter;save();render();}));
  app.querySelectorAll("[data-channel]").forEach((button)=>button.addEventListener("click",()=>{state.activeChannel=button.dataset.channel;save();render();toast(`#${button.textContent.trim().replace(/^#\s*/,"")} opened`,`Circle channel`);}));
  app.querySelectorAll("[data-open-circle]").forEach((button)=>button.addEventListener("click",()=>{state.activeConversation=button.dataset.openCircle;state.activeChannel="lobby";state.route="chats";state.mobileChatOpen=true;save();render();}));
  const search=app.querySelector("#chat-search"); search?.addEventListener("input",()=>{state.query=search.value;save();render();requestAnimationFrame(()=>{const next=document.querySelector("#chat-search");next?.focus();next?.setSelectionRange(state.query.length,state.query.length);});});
  app.querySelector("#message-form")?.addEventListener("submit",(event)=>{event.preventDefault();sendMessage();});
  app.querySelector("#message-input")?.addEventListener("keydown",(event)=>{if(event.key==="Enter"&&!event.shiftKey&&state.preferences.enterToSend){event.preventDefault();sendMessage();}});
  app.querySelectorAll("[data-export]").forEach((button)=>button.addEventListener("click",()=>exportData(button.dataset.export)));
  app.querySelectorAll("[data-theme-choice]").forEach((button)=>button.addEventListener("click",()=>{state.appearance.theme=button.dataset.themeChoice;save();render();}));
  app.querySelectorAll("[data-accent]").forEach((button)=>button.addEventListener("click",()=>{state.appearance.accent=button.dataset.accent;save();render();}));
  app.querySelectorAll("[data-pref]").forEach((button)=>button.addEventListener("click",()=>{state.preferences[button.dataset.pref]=!state.preferences[button.dataset.pref];save();render();}));
  app.querySelectorAll("[data-bot-toggle]").forEach((button)=>button.addEventListener("click",()=>{const bot=state.bots.find(item=>item.id===button.dataset.botToggle);bot.status=bot.status==="running"?"paused":"running";addEvent("project.status",`${bot.id} · ${bot.status}`);save();render();}));
  app.querySelectorAll("[data-automation-toggle]").forEach((button)=>button.addEventListener("click",()=>{const item=state.automations.find(value=>value.id===button.dataset.automationToggle);item.enabled=!item.enabled;save();render();}));
  app.querySelector("#density")?.addEventListener("change",(event)=>{state.appearance.density=event.target.value;save();render();});
  app.querySelector("#message-shape")?.addEventListener("change",(event)=>{state.appearance.messageShape=event.target.value;save();render();});
  app.querySelector("#font-scale")?.addEventListener("input",(event)=>{state.appearance.fontScale=Number(event.target.value);save();applyAppearance();});
  app.querySelector("#receipt-target")?.addEventListener("change",(event)=>{state.storage.receiptTarget=event.target.value;save();});
  app.querySelector("#relay-expiry")?.addEventListener("change",(event)=>{state.storage.freeExpiry=event.target.value;save();});
  app.querySelectorAll("[data-action]").forEach((button)=>button.addEventListener("click",()=>handleAction(button.dataset.action,button)));
}

function handleAction(action, element) {
  if (action === "new-chat" || action === "friend-card") return friendModal();
  if (action === "new-circle") return circleModal();
  if (action === "create-bot") return projectModal();
  if (action === "whiteboard") return whiteboardModal();
  if (action === "call") return callModal(false);
  if (action === "video-call") return callModal(true);
  if (action === "timer" || action === "view-once" || action === "message-rules") return messageRulesModal(element.dataset.message);
  if (action === "security") return showModal("Encryption details",`<div class="security-card"><strong>${icons.lock} Verified E2EE session</strong><p>Messages, calls, canvases, reactions and attachments use per-device keys. The service relays ciphertext and cannot read content.</p></div><div class="list modal-note"><div class="list-row"><span class="list-copy"><strong>Current key epoch</strong><span>7 · rotated 2 days ago</span></span><span class="badge live">Healthy</span></div><div class="list-row"><span class="list-copy"><strong>Safety number</strong><span>72 18 44 90 · verified locally</span></span><button class="secondary-button">Compare</button></div></div>`);
  if (action === "reply") { state.replyTo=element.dataset.message;save();render();document.querySelector("#message-input")?.focus();return; }
  if (action === "cancel-reply") { state.replyTo=null;save();render();return; }
  if (action === "quick-react" || action === "react") { const messages=state.messages[state.activeConversation];const message=messages.find(item=>item.id===element.dataset.message);message.reactions??=[];let reaction=message.reactions.find(item=>item.e===(element.dataset.emoji??"♡"));if(reaction){reaction.mine=!reaction.mine;reaction.n+=reaction.mine?1:-1;}else message.reactions.push({e:"♡",n:1,mine:true});save();render();return; }
  if (action === "scrub-message") { const list=state.messages[state.activeConversation];const message=list.find(item=>item.id===element.dataset.message);message.text="Message scrubbed by its author";delete message.attachment;message.scrubbed=true;addEvent("message.scrubbed",`${state.activeConversation}/${message.id} · author action`);save();render();return; }
  if (action === "mark-read") { state.conversations.forEach(item=>item.unread=0);save();render();return; }
  if (action === "mobile-back") { state.mobileChatOpen=false;save();render();return; }
  if (action === "toggle-generic") { element.classList.toggle("on");return; }
  if (action === "toggle-plan") { state.storage.plan=state.storage.plan==="pro"?"free":"pro";state.storage.hostedUsed=state.storage.plan==="pro"?124.8:0;save();render();return; }
  if (action === "emit-event") { addEvent("plugin.test",`evt_${Math.random().toString(16).slice(2,8)} · local sandbox`);save();render();return; }
  if (action === "copy-code") { navigator.clipboard?.writeText(document.querySelector("#sdk-code")?.textContent??"");toast("SDK starter copied");return; }
  if (action === "choose-avatar") { avatarFile.click();return; }
  if (action === "save-profile") { state.profile.name=document.querySelector("#profile-name").value;state.profile.username=document.querySelector("#profile-handle").value;state.profile.status=document.querySelector("#profile-status").value;state.profile.avatarUrl=document.querySelector("#avatar-url").value;save();render();toast("Profile saved locally");return; }
  if (action === "reset") { localStorage.removeItem(STORAGE_KEY);state=cloneSeed();render();toast("Prototype data reset");return; }
  if (action === "attach" || action === "mock-download" || action === "custom-css" || action === "new-automation") return toast("Interactive concept ready for product wiring",action.replaceAll("-"," "));
  if (action === "open-once") { element.textContent="Opened · closes in 12s";element.disabled=true;return; }
}

avatarFile.addEventListener("change",()=>{ const file=avatarFile.files?.[0]; if(!file)return; const reader=new FileReader(); reader.onload=()=>{state.profile.avatarUrl=String(reader.result);save();render();toast(`${file.type.includes("gif")?"Animated":"Local"} avatar applied`);}; reader.readAsDataURL(file); });
document.addEventListener("click",(event)=>{ const target=event.target.closest("[data-action]"); if(!target||app.contains(target))return; const action=target.dataset.action;
  if(action==="close-modal")return closeModal();
  if(action==="clear-canvas"){const canvas=document.querySelector("#whiteboard");canvas?.getContext("2d").clearRect(0,0,canvas.width,canvas.height);return;}
  if(action==="share-canvas"){closeModal();const conversation=getConversation();(state.messages[conversation.id]??=[]).push({id:`canvas-${Date.now()}`,from:"me",at:new Date().toISOString(),text:"Shared live canvas",attachment:{name:"Collaborative canvas",size:"Live · encrypted",type:"CANVAS",state:"Displayed in chat"}});save();render();toast("Canvas displayed in this chat");return;}
  if(action==="start-call"){closeModal();toast("Encrypted room started · prototype","Call ready");return;}
  if(action==="send-friend-request"){closeModal();toast("Encrypted friend request created locally");return;}
  if(action==="create-circle-confirm"){const name=document.querySelector("#circle-name")?.value.trim()||"New Circle";closeModal();toast(`${name} created in the local prototype`);return;}
  if(action==="create-project-confirm"){const name=document.querySelector("#project-name")?.value.trim()||"New project";closeModal();toast(`${name} sandbox created locally`);return;}
  if(target.dataset.applyRules){const message=state.messages[state.activeConversation]?.find(item=>item.id===target.dataset.applyRules);const timer=document.querySelector("#rule-timer")?.value;if(message&&timer&&timer!=="off")message.timer=timer;closeModal();save();render();toast("Encrypted message rule applied");}
});
document.addEventListener("keydown",(event)=>{if(event.key==="Escape")closeModal();});

render();
