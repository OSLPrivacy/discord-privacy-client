import { futureAccountSwitchMarkup } from "./future-account-switch";

const personId = "hub-person-ada";
const app = document.querySelector("#app")!;
let enabled = true;

function render(): void {
  app.innerHTML = `<main class="content-viewport" data-task-0269-friend-page data-task-0269-state="${enabled ? "on" : "off"}" aria-labelledby="task-0269-heading"><header class="destination-header"><div><p class="eyebrow">People</p><h1 id="task-0269-heading" tabindex="-1">Friend</h1><p>Review the chat permissions shared with this person.</p></div></header><section class="home-people-list" aria-label="Friend page fixture"><article class="person-row person-profile"><header><div><strong>Ada Lovelace</strong><small>Verified friend</small></div><button class="button compact" type="button">Message</button></header><details class="friend-management" open><summary>Manage</summary><div><div class="friend-approvals"><span>Approved chats</span><div><span class="friend-scope">OSL Chat</span></div></div>${futureAccountSwitchMarkup({ personId, enabled })}<details class="friend-security"><summary>Security details</summary><div><span>OSL ID</span><code>osl:ada-lovelace</code></div></details></div></details></article></section></main>`;
  app.querySelector<HTMLInputElement>("[data-future-account-toggle]")!.addEventListener("change", (event) => {
    enabled = (event.currentTarget as HTMLInputElement).checked;
    render();
  });
}

render();
