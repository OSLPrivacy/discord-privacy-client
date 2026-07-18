"use strict";

// OSL Privacy interaction prototype. This file intentionally has no networking,
// credential handling, platform automation, or production cryptography.
(function () {
  const services = [
    {
      id: "discord",
      name: "Discord",
      initial: "D",
      accounts: [
        { id: "discord-personal", label: "Personal", handle: "@rose.test", recipient: "Rose", protection: "verified" },
        { id: "discord-osl", label: "OSL testing", handle: "@osl.test", recipient: "OSL test", protection: "unverified" },
      ],
    },
    {
      id: "telegram",
      name: "Telegram",
      initial: "T",
      accounts: [
        { id: "telegram-personal", label: "Personal", handle: "@rose_private", recipient: "Rose", protection: "verified" },
        { id: "telegram-project", label: "Project", handle: "@osl_project", recipient: "Project room", protection: "unsupported" },
      ],
    },
    {
      id: "instagram",
      name: "Instagram",
      initial: "I",
      accounts: [
        { id: "instagram-personal", label: "Personal", handle: "@rose.private", recipient: "Rose", protection: "verified" },
        { id: "instagram-work", label: "Work", handle: "@oslprivacy", recipient: "Work contact", protection: "unverified" },
      ],
    },
    {
      id: "snapchat",
      name: "Snapchat",
      initial: "S",
      accounts: [
        { id: "snapchat-personal", label: "Personal", handle: "rose.test", recipient: "Rose", protection: "verified" },
        { id: "snapchat-testing", label: "Testing", handle: "osl.test", recipient: "Test contact", protection: "unsupported" },
      ],
    },
    {
      id: "email",
      name: "Email providers",
      initial: "E",
      accounts: [
        { id: "email-personal", label: "Personal", handle: "rose@example.test", recipient: "Rose", protection: "verified" },
        { id: "email-work", label: "Work", handle: "osl@example.test", recipient: "Work contact", protection: "unverified" },
        { id: "email-private", label: "Private", handle: "private@example.test", recipient: "External contact", protection: "unsupported" },
      ],
    },
    {
      id: "x",
      name: "X",
      initial: "X",
      accounts: [
        { id: "x-personal", label: "Personal", handle: "@rose_test", recipient: "Rose", protection: "verified" },
        { id: "x-osl", label: "OSL", handle: "@oslprivacy", recipient: "OSL contact", protection: "unverified" },
      ],
    },
    {
      id: "slack",
      name: "Slack",
      initial: "S",
      accounts: [
        { id: "slack-community", label: "Community", handle: "rose@privacy-lab", recipient: "Rose", protection: "verified" },
        { id: "slack-work", label: "Work", handle: "osl@workspace", recipient: "Workspace channel", protection: "unsupported" },
      ],
    },
    {
      id: "teams",
      name: "Teams",
      initial: "T",
      accounts: [
        { id: "teams-personal", label: "Personal", handle: "rose@tenant.test", recipient: "Rose", protection: "verified" },
        { id: "teams-work", label: "Work", handle: "osl@tenant.test", recipient: "Tenant contact", protection: "unverified" },
      ],
    },
    {
      id: "messenger",
      name: "Facebook Messenger",
      initial: "F",
      accounts: [
        { id: "messenger-personal", label: "Personal", handle: "Rose Test", recipient: "Rose", protection: "verified" },
        { id: "messenger-page", label: "OSL page", handle: "OSL Privacy Test", recipient: "Page visitor", protection: "unsupported" },
      ],
    },
  ];

  const pageTitles = {
    home: ["OSL Privacy", "Good evening, Liam"],
    inbox: ["Private communication", "Inbox"],
    people: ["Identity and trust", "People"],
    composer: ["Protection workspace", "Secure Composer"],
    privacy: ["Local privacy tools", "Privacy"],
    connections: ["Accounts and services", "Connections"],
    activity: ["Local evidence", "Activity"],
    settings: ["OSL Privacy", "Settings"],
  };

  const timers = ["Off", "1 day", "3 days", "7 days"];
  const drafts = new Map();
  const conversationModes = new Map();
  const state = {
    serviceId: "discord",
    accountId: "discord-personal",
    tier: "free",
    timerIndex: 0,
    capability: "high",
    challengePaused: false,
    recoveryTimer: null,
    capsuleCount: 0,
    mode: "protected",
  };

  const byId = (id) => document.getElementById(id);
  const currentService = () => services.find((service) => service.id === state.serviceId);
  const currentAccount = () => currentService().accounts.find((account) => account.id === state.accountId);
  const identityText = () => `${currentService().name} · ${currentAccount().label} · ${currentAccount().handle}`;
  const protectedEligible = () => currentAccount().protection === "verified";
  const draftKey = (mode = state.mode) => `${state.accountId}:${mode}`;

  function escapeHtml(value) {
    return String(value)
      .replaceAll("&", "&amp;")
      .replaceAll("<", "&lt;")
      .replaceAll(">", "&gt;")
      .replaceAll('"', "&quot;")
      .replaceAll("'", "&#039;");
  }

  function showToast(message) {
    const toast = document.createElement("div");
    toast.className = "toast";
    toast.textContent = message;
    byId("toast-region").append(toast);
    window.setTimeout(() => toast.remove(), 3200);
  }

  function navigate(pageName, focusHeading = false) {
    const targetPage = document.querySelector(`[data-page="${pageName}"]`);
    if (!targetPage) return;

    document.querySelectorAll(".page").forEach((page) => {
      page.classList.toggle("active", page === targetPage);
    });
    document.querySelectorAll(".nav-item[data-page-target]").forEach((button) => {
      const active = button.dataset.pageTarget === pageName;
      button.classList.toggle("active", active);
      if (active) button.setAttribute("aria-current", "page");
      else button.removeAttribute("aria-current");
    });

    const [eyebrow, title] = pageTitles[pageName];
    byId("page-eyebrow").textContent = eyebrow;
    byId("page-title").textContent = title;
    if (focusHeading) {
      byId("main-content").focus({ preventScroll: true });
    }
  }

  function renderHomeServices() {
    byId("home-services").innerHTML = services
      .map(
        (service) => `
          <article class="mini-service">
            <span class="service-avatar" aria-hidden="true">${escapeHtml(service.initial)}</span>
            <strong>${escapeHtml(service.name)}</strong>
            <small>${service.accounts.length} test account${service.accounts.length === 1 ? "" : "s"}</small>
          </article>`,
      )
      .join("");
  }

  function renderConversations() {
    const rows = [
      ["R", "Rose", "did the prototype work?"],
      ["O", "OSL test", "identity review waiting"],
      ["P", "Privacy lab", "local-only draft"],
    ];
    byId("conversation-list").innerHTML = rows
      .map(
        ([initial, name, preview]) => `
          <button class="conversation-row" type="button">
            <span class="avatar" aria-hidden="true">${initial}</span>
            <span><strong>${name}</strong><small>${preview}</small></span>
            <small>Simulated</small>
          </button>`,
      )
      .join("");
  }

  function renderConnections() {
    byId("connections-grid").innerHTML = services
      .map(
        (service, index) => `
          <article class="connection-card${index === 0 ? " expanded" : ""}" data-service-card="${service.id}">
            <button class="connection-head" type="button" aria-expanded="${index === 0}" aria-controls="accounts-${service.id}">
              <span class="service-avatar" aria-hidden="true">${escapeHtml(service.initial)}</span>
              <span><strong>${escapeHtml(service.name)}</strong><small>${service.accounts.length} test accounts · Native + OSL modes</small></span>
              <span class="chevron" aria-hidden="true">⌄</span>
            </button>
            <div class="account-rows" id="accounts-${service.id}">
              ${service.accounts
                .map(
                  (account) => `
                    <div class="account-row" data-testid="account-row-${service.id}-${account.id}">
                      <div><strong>${escapeHtml(account.label)}</strong><small>${escapeHtml(account.handle)} · ${account.protection === "verified" ? "Verified OSL recipient" : "Native fallback"}</small></div>
                      <button class="secondary-button account-switch" type="button" data-service="${service.id}" data-account="${account.id}" aria-label="Switch to ${escapeHtml(service.name)} ${escapeHtml(account.label)}">Switch</button>
                    </div>`,
                )
                .join("")}
            </div>
          </article>`,
      )
      .join("");

    document.querySelectorAll(".connection-head").forEach((button) => {
      button.addEventListener("click", () => {
        const card = button.closest(".connection-card");
        const expanded = card.classList.toggle("expanded");
        button.setAttribute("aria-expanded", String(expanded));
      });
    });
    document.querySelectorAll(".account-switch").forEach((button) => {
      button.addEventListener("click", () => {
        switchIdentity(button.dataset.service, button.dataset.account);
        navigate("composer");
        byId("secure-text").focus();
        showToast(`Switched to ${identityText()}. Only this account's draft is shown.`);
      });
    });
  }

  function populateServiceSelect() {
    byId("service-select").innerHTML = services
      .map((service) => `<option value="${service.id}">${escapeHtml(service.name)}</option>`)
      .join("");
    byId("service-select").value = state.serviceId;
  }

  function populateAccountSelect() {
    const service = currentService();
    byId("account-select").innerHTML = service.accounts
      .map(
        (account) => `<option value="${account.id}">${escapeHtml(account.label)} · ${escapeHtml(account.handle)}</option>`,
      )
      .join("");
    byId("account-select").value = state.accountId;
  }

  function saveCurrentDraft() {
    const composer = state.mode === "protected" ? byId("secure-text") : byId("native-composer");
    drafts.set(draftKey(), composer.value);
  }

  function clearHandoff() {
    if (state.mode === "protected") {
      byId("native-composer").value = "";
      byId("native-send").disabled = true;
      byId("handoff-status").textContent = "Plaintext has not entered the platform.";
      return;
    }

    byId("native-send").disabled = !byId("native-composer").value.trim();
    byId("handoff-status").textContent = "Native draft uses the service's ordinary send path and security.";
  }

  function switchIdentity(serviceId, accountId) {
    saveCurrentDraft();
    window.clearTimeout(state.recoveryTimer);
    state.serviceId = serviceId;
    const service = currentService();
    state.accountId = service.accounts.some((account) => account.id === accountId)
      ? accountId
      : service.accounts[0].id;

    populateServiceSelect();
    populateAccountSelect();
    state.mode = conversationModes.get(state.accountId) || (protectedEligible() ? "protected" : "native");
    conversationModes.set(state.accountId, state.mode);
    updateIdentityUi();
    restoreCapability(false);
  }

  function updateIdentityUi() {
    const service = currentService();
    const account = currentAccount();
    byId("header-avatar").textContent = service.initial;
    byId("header-account").textContent = identityText();
    byId("mock-avatar").textContent = service.initial;
    byId("mock-service-name").textContent = service.name;
    byId("mock-account-name").textContent = `${account.label} · ${account.handle}`;
    byId("scan-scope").textContent = identityText();
  }

  function setTier(tier) {
    state.tier = tier;
    document.body.dataset.tier = tier;
    document.querySelectorAll(".tier-button").forEach((button) => {
      const active = button.dataset.tier === tier;
      button.classList.toggle("active", active);
      button.setAttribute("aria-pressed", String(active));
    });
    if (tier === "free") {
      byId("media-toggle").classList.remove("active");
      byId("media-toggle").setAttribute("aria-pressed", "false");
    }
    showToast(tier === "pro" ? "Pro preview enabled. No purchase or account change was made." : "Free preview enabled.");
  }

  function showProDialog() {
    byId("pro-dialog").showModal();
  }

  function toggleTheme() {
    const html = document.documentElement;
    const next = html.dataset.theme === "dark" ? "light" : "dark";
    html.dataset.theme = next;
    byId("theme-toggle").setAttribute("aria-label", `Switch to ${next === "dark" ? "light" : "dark"} theme`);
    showToast(`${next[0].toUpperCase()}${next.slice(1)} theme preview enabled.`);
  }

  function updateToggle(button, active) {
    button.classList.toggle("active", active);
    button.setAttribute("aria-pressed", String(active));
  }

  function protectionReason() {
    if (currentAccount().protection === "unverified") {
      return `${currentAccount().recipient} has an OSL identity, but it is not verified for this simulated conversation.`;
    }
    return `${currentAccount().recipient} is not simulated as OSL-capable on this account.`;
  }

  function updateComposerMode() {
    const isProtected = state.mode === "protected";
    const eligible = protectedEligible();
    const nativeComposer = byId("native-composer");
    const secureComposer = byId("secure-composer-box");

    document.querySelectorAll('input[name="conversation-mode"]').forEach((radio) => {
      radio.checked = radio.value === state.mode;
    });
    byId("mode-native-option").classList.toggle("selected", !isProtected);
    byId("mode-protected-option").classList.toggle("selected", isProtected);
    byId("mode-protected-option").classList.toggle("unavailable", !eligible);

    secureComposer.hidden = !isProtected;
    nativeComposer.disabled = isProtected;
    byId("platform-stage").classList.toggle("native-mode", !isProtected);

    if (isProtected) {
      byId("secure-text").value = drafts.get(draftKey("protected")) || "";
      setComposerAvailability(state.capability === "high" && !state.challengePaused);
      setLayoutLabel("good", "Layout matched · 98%");
      byId("mode-explanation").textContent = `True E2EE is simulated between this device and verified OSL recipient ${currentAccount().recipient}. ${currentService().name} only carries an opaque capsule.`;
      byId("capability-state").innerHTML = '<span class="status-dot good"></span> Verified OSL recipient · user-assisted send';
      byId("capability-copy").textContent = `OSL Protected simulates E2EE to ${currentAccount().recipient}. You press the platform Send button; ordinary ${currentService().name} messages are not OSL-encrypted.`;
    } else {
      // Do not leave the previous account's protected plaintext in a hidden DOM
      // control after switching to a Native-only identity.
      byId("secure-text").value = "";
      nativeComposer.value = drafts.get(draftKey("native")) || "";
      nativeComposer.placeholder = `Message with ${currentService().name} natively...`;
      setComposerAvailability(false);
      setLayoutLabel("good", "Native service · available");
      if (eligible) {
        byId("mode-explanation").textContent = `Native keeps all ${currentService().name} features. This message uses the service's own security, not OSL E2EE.`;
        byId("capability-state").innerHTML = '<span class="status-dot good"></span> Native service mode';
        byId("capability-copy").textContent = `Full native ${currentService().name} functionality remains available. Switch to OSL Protected for verified OSL E2EE.`;
      } else {
        byId("mode-explanation").textContent = `${protectionReason()} Staying in Native; OSL does not relabel an ordinary platform message as E2EE.`;
        byId("capability-state").innerHTML = '<span class="status-dot warn"></span> Native · user-assisted OSL';
        byId("capability-copy").textContent = `Native ${currentService().name} remains available with its own features and security. OSL can assist with local checks; you perform the final platform action.`;
      }
    }
    clearHandoff();
  }

  function selectConversationMode(requestedMode) {
    saveCurrentDraft();
    if (requestedMode === "protected" && !protectedEligible()) {
      state.mode = "native";
      conversationModes.set(state.accountId, "native");
      updateComposerMode();
      showToast(`${protectionReason()} Kept this conversation in Native.`);
      return;
    }

    state.mode = requestedMode;
    conversationModes.set(state.accountId, requestedMode);
    updateComposerMode();
    showToast(requestedMode === "protected"
      ? "OSL Protected selected for this verified simulated recipient. Nothing was sent."
      : `Native ${currentService().name} selected. Full service features remain available.`);
  }

  function protectAndHandoff() {
    if (state.mode !== "protected" || !protectedEligible()) {
      selectConversationMode("native");
      return;
    }
    const draft = byId("secure-text").value.trim();
    if (!draft) {
      showToast("Type a simulated draft before protecting it.");
      byId("secure-text").focus();
      return;
    }
    if (state.capability !== "high" || state.challengePaused) {
      showToast("Handoff is paused. Restore a safe composer placement first.");
      return;
    }

    state.capsuleCount += 1;
    const opaqueCapsule = `OSL1:SIMULATED:${String(state.capsuleCount).padStart(4, "0")}:7FQ9K2`;
    byId("native-composer").value = opaqueCapsule;
    byId("native-send").disabled = false;
    byId("handoff-status").textContent = "Simulated E2EE capsule prepared. You still choose Native Send.";
    showToast("OSL E2EE capsule prepared locally for the verified simulated recipient. Nothing was sent.");
  }

  function simulateNativeSend() {
    if (byId("native-send").disabled) return;
    const bubble = document.createElement("p");
    bubble.className = "bubble outgoing";
    bubble.textContent = state.mode === "protected"
      ? "OSL Protected · simulated E2EE capsule"
      : `${currentService().name} Native · simulated message`;
    byId("sent-messages").append(bubble);
    drafts.set(draftKey(), "");
    if (state.mode === "protected") byId("secure-text").value = "";
    else byId("native-composer").value = "";
    clearHandoff();
    showToast(state.mode === "protected"
      ? "Simulated capsule handoff complete. No platform was contacted."
      : "Simulated native send complete. No platform was contacted.");
  }

  function setComposerAvailability(enabled) {
    byId("secure-text").disabled = !enabled;
    byId("handoff-button").disabled = !enabled;
    byId("check-button").disabled = !enabled;
  }

  function setLayoutLabel(kind, text) {
    const stateLabel = byId("layout-state");
    stateLabel.innerHTML = `<span class="status-dot ${kind}"></span> ${escapeHtml(text)}`;
  }

  function restoreCapability(announce = true) {
    state.capability = "high";
    state.challengePaused = false;
    byId("secure-composer-box").classList.remove("medium-confidence", "low-confidence");
    byId("low-confidence").textContent = "Simulate low confidence";
    byId("platform-challenge").textContent = "Simulate platform challenge";
    updateComposerMode();
    if (announce) showToast("Safe composer placement restored. Draft preserved.");
  }

  function simulateLayoutChange() {
    if (state.mode !== "protected") {
      showToast("Layout recovery applies to OSL Protected. Native service mode is unchanged.");
      return;
    }
    if (state.challengePaused) {
      showToast("Restore from the platform challenge before testing layout recovery.");
      return;
    }
    window.clearTimeout(state.recoveryTimer);
    state.capability = "medium";
    clearHandoff();
    byId("secure-composer-box").classList.remove("low-confidence");
    byId("secure-composer-box").classList.add("medium-confidence");
    setComposerAvailability(false);
    setLayoutLabel("warn", "Layout changed · repairing safely");
    byId("capability-state").innerHTML = '<span class="status-dot warn"></span> Sidecar fallback';
    byId("capability-copy").textContent = "Handoff is paused while local structural anchors are checked.";
    showToast("Layout change detected. Draft preserved; handoff paused.");
    state.recoveryTimer = window.setTimeout(() => {
      restoreCapability(false);
      showToast("Layout recovered at high confidence. Draft preserved.");
    }, 1300);
  }

  function simulateLowConfidence() {
    if (state.mode !== "protected") {
      showToast("Placement confidence applies to OSL Protected. Native service mode is unchanged.");
      return;
    }
    if (state.capability === "low") {
      restoreCapability();
      return;
    }
    window.clearTimeout(state.recoveryTimer);
    state.capability = "low";
    clearHandoff();
    byId("secure-composer-box").classList.remove("medium-confidence");
    byId("secure-composer-box").classList.add("low-confidence");
    setComposerAvailability(false);
    setLayoutLabel("warn", "Layout changed - review placement");
    byId("capability-state").innerHTML = '<span class="status-dot warn"></span> Capability paused';
    byId("capability-copy").textContent = "Confidence is too low. OSL will not hand text to the platform.";
    byId("low-confidence").textContent = "Restore safe sidecar";
    showToast("Low confidence: affected capability disabled. Draft preserved.");
  }

  function simulatePlatformChallenge() {
    if (state.mode !== "protected") {
      showToast("OSL does not interact with platform challenges in Native mode.");
      return;
    }
    if (state.challengePaused) {
      restoreCapability();
      return;
    }
    window.clearTimeout(state.recoveryTimer);
    state.challengePaused = true;
    state.capability = "paused";
    clearHandoff();
    byId("secure-composer-box").classList.add("low-confidence");
    setComposerAvailability(false);
    setLayoutLabel("warn", "Platform challenge - user action required");
    byId("capability-state").innerHTML = '<span class="status-dot warn"></span> Platform challenge';
    byId("capability-copy").textContent = "OSL paused the composer. Resolve the platform prompt yourself; no automated response occurs.";
    byId("platform-challenge").textContent = "Mark challenge resolved";
    showToast("Composer paused safely. Draft preserved; OSL does not interact with challenges.");
  }

  function runScan() {
    const findings = [
      ["Possible address", "A simulated old message may reveal a home address."],
      ["Travel detail", "A simulated conversation may disclose when a home is empty."],
      ["Account recovery clue", "A simulated message may contain a memorable recovery answer."],
    ];
    const results = byId("scan-results");
    results.hidden = false;
    results.innerHTML = `
      <div><strong>3 items may reveal your home address or private routines</strong><p class="field-note">Review calmly. Nothing was deleted or changed.</p></div>
      ${findings
        .map(
          ([title, copy], index) => `
            <article class="finding" data-finding="${index}">
              <strong>${escapeHtml(title)}</strong>
              <p>${escapeHtml(copy)}</p>
              <div class="finding-actions">
                <button class="secondary-button finding-open" type="button">Open message</button>
                <button class="secondary-button finding-steps" type="button">Show deletion steps <span class="pro-label">Pro</span></button>
                <button class="text-button finding-ignore" type="button">Ignore</button>
              </div>
            </article>`,
        )
        .join("")}`;

    results.querySelectorAll(".finding-open").forEach((button) => {
      button.addEventListener("click", () => showToast("Simulated message location opened. No account was accessed."));
    });
    results.querySelectorAll(".finding-steps").forEach((button) => {
      button.addEventListener("click", () => {
        if (state.tier === "free") showProDialog();
        else showToast("Guided deletion steps previewed. You remain in control of every platform action.");
      });
    });
    results.querySelectorAll(".finding-ignore").forEach((button) => {
      button.addEventListener("click", () => {
        button.closest(".finding").remove();
        showToast("Finding ignored locally. No platform data changed.");
      });
    });
    showToast("Local simulated scan complete: 3 review items, 0 removals.");
  }

  function bindEvents() {
    document.querySelectorAll("[data-page-target]").forEach((button) => {
      button.addEventListener("click", () => navigate(button.dataset.pageTarget, true));
    });
    document.querySelectorAll("[data-go]").forEach((button) => {
      button.addEventListener("click", () => {
        navigate(button.dataset.go, true);
        if (button.dataset.triggerScan === "true") runScan();
      });
    });
    document.querySelectorAll(".tier-button").forEach((button) => {
      button.addEventListener("click", () => setTier(button.dataset.tier));
    });

    byId("theme-toggle").addEventListener("click", toggleTheme);
    byId("settings-theme").addEventListener("click", toggleTheme);

    byId("service-select").addEventListener("change", (event) => {
      const service = services.find((item) => item.id === event.target.value);
      switchIdentity(service.id, service.accounts[0].id);
      showToast(`Switched to ${identityText()}. Only this account's draft is shown.`);
    });
    byId("account-select").addEventListener("change", (event) => {
      switchIdentity(state.serviceId, event.target.value);
      showToast(`Switched to ${identityText()}. Only this account's draft is shown.`);
    });
    byId("secure-text").addEventListener("input", (event) => {
      drafts.set(draftKey("protected"), event.target.value);
      clearHandoff();
    });
    byId("native-composer").addEventListener("input", (event) => {
      drafts.set(draftKey("native"), event.target.value);
      clearHandoff();
    });
    document.querySelectorAll('input[name="conversation-mode"]').forEach((radio) => {
      radio.addEventListener("change", (event) => selectConversationMode(event.target.value));
    });
    byId("translate-toggle").addEventListener("click", (event) => {
      const next = event.currentTarget.getAttribute("aria-pressed") !== "true";
      updateToggle(event.currentTarget, next);
      showToast(next ? "Local translation preview on." : "Local translation preview off.");
    });
    byId("timer-toggle").addEventListener("click", (event) => {
      state.timerIndex = (state.timerIndex + 1) % timers.length;
      const timer = timers[state.timerIndex];
      event.currentTarget.textContent = `Timer: ${timer}`;
      updateToggle(event.currentTarget, timer !== "Off");
      showToast(`Simulated expiry set to ${timer}.`);
    });
    byId("media-toggle").addEventListener("click", (event) => {
      if (state.tier === "free") {
        showProDialog();
        return;
      }
      const next = event.currentTarget.getAttribute("aria-pressed") !== "true";
      updateToggle(event.currentTarget, next);
      showToast(next ? "Simulated protected image selected." : "Simulated image removed.");
    });
    byId("handoff-button").addEventListener("click", protectAndHandoff);
    byId("native-send").addEventListener("click", simulateNativeSend);
    byId("check-button").addEventListener("click", () => byId("warning-dialog").showModal());

    byId("composer-collapse").addEventListener("click", (event) => {
      const button = event.currentTarget;
      const expanded = button.getAttribute("aria-expanded") === "true";
      button.setAttribute("aria-expanded", String(!expanded));
      byId("composer-detail").hidden = expanded;
      document.querySelector(".quick-toggles").hidden = expanded;
    });

    byId("layout-change").addEventListener("click", simulateLayoutChange);
    byId("low-confidence").addEventListener("click", simulateLowConfidence);
    byId("platform-challenge").addEventListener("click", simulatePlatformChallenge);
    byId("scan-button").addEventListener("click", runScan);

    document.querySelectorAll('input[name="preset"]').forEach((radio) => {
      radio.addEventListener("change", () => {
        document.querySelectorAll(".preset-card").forEach((card) => {
          card.classList.toggle("selected", card.contains(radio));
        });
        showToast(`${radio.value[0].toUpperCase()}${radio.value.slice(1)} preset selected locally.`);
      });
    });

    document.querySelectorAll(".pro-action").forEach((button) => {
      button.addEventListener("click", showProDialog);
    });
    byId("dialog-pro-preview").addEventListener("click", () => setTier("pro"));
    byId("warning-dialog").addEventListener("close", () => {
      if (byId("warning-dialog").returnValue === "send") {
        showToast("Warning acknowledged. Nothing was sent.");
      }
    });
  }

  function init() {
    renderHomeServices();
    renderConversations();
    renderConnections();
    populateServiceSelect();
    populateAccountSelect();
    updateIdentityUi();
    bindEvents();
    conversationModes.set(state.accountId, state.mode);
    restoreCapability(false);
    document.body.dataset.tier = state.tier;
  }

  init();
})();
