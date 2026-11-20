// Click-through navigation for the static mockups. Each screen is a separate
// HTML file; this wires sidebar, breadcrumbs, buttons and rows to them.
(function () {
  const P = {
    system: "00-design-system.html", landing: "01-landing-page.html", connect: "02-connect-wallet.html",
    dashboard: "03-dashboard.html", escrow: "04-escrow-list.html", escrowNew: "05-escrow-create.html",
    escrowDetail: "06-escrow-detail.html", dispute: "07-escrow-resolve-dispute.html", vesting: "08-vesting.html",
    multisig: "09-multisig-wallet.html", dao: "10-dao-proposals.html", proposal: "11-dao-proposal-vote.html",
    subs: "12-subscriptions.html", royalties: "13-marketplace-royalties.html", deploy: "14-deploy-templates.html",
    activity: "15-activity-events.html", dev: "16-cli-and-sdk.html", settings: "17-settings.html",
    mobile: "18-mobile-screens.html", tx: "19-transaction-states.html", index: "index.html",
  };
  const here = location.pathname.split("/").pop() || "index.html";
  const text = (el) => el.textContent.replace(/\s+/g, " ").trim();

  function go(el, href) {
    if (!el || el.dataset.nav) return;
    el.dataset.nav = href;
    el.style.cursor = "pointer";
    el.addEventListener("click", (e) => { e.stopPropagation(); location.href = href; });
  }
  // Link every element matching `sel` whose text matches a rule: [regex, page].
  function wire(sel, rules, root = document) {
    root.querySelectorAll(sel).forEach((el) => {
      const t = text(el);
      for (const [re, href] of rules) if (re.test(t)) { go(el, P[href] || href); break; }
    });
  }

  // App shell: logo, sidebar, header.
  document.querySelectorAll(".logo").forEach((el) => go(el, P.landing));
  wire(".side .nav", [
    [/^Dashboard/, "dashboard"], [/^Activity/, "activity"], [/^Escrow/, "escrow"], [/^Vesting/, "vesting"],
    [/^Multi-sig/, "multisig"], [/^DAO/, "dao"], [/^Subscriptions/, "subs"], [/^Royalties/, "royalties"],
    [/^Deploy/, "deploy"], [/^CLI/, "dev"], [/^Settings/, "settings"],
  ]);
  const top = document.querySelector("header.top");
  if (top) {
    const btns = top.querySelectorAll(".btn");
    go(btns[0], P.activity);          // bell
    go(btns[1], P.settings);          // network switcher
    go(btns[2], P.connect);           // wallet
  }

  // Breadcrumbs: each segment links to its screen.
  const crumbPages = { Contracts: "dashboard", Overview: "dashboard", Build: "deploy", Settings: "settings",
    Escrow: "escrow", "Multi-sig": "multisig", "DAO Governance": "dao" };
  document.querySelectorAll(".crumbs").forEach((c) => {
    c.innerHTML = c.innerHTML.replace(/(^|<\/span>)\s*([^<]+?)\s*(?=<span>)/g, (m, pre, label) =>
      crumbPages[label] ? `${pre} <a class="crumb-link" href="${P[crumbPages[label]]}">${label}</a> ` : m);
  });

  // Buttons and links, by label.
  wire(".btn, .ember, .small.ember", [
    [/^Launch app/, "connect"], [/^Deploy a contract/, "deploy"], [/^Read the docs|^Docs$/, "dev"],
    [/^View feature matrix|^Compare features/, "deploy"],
    [/^New contract|^Sign upload/, "deploy"], [/^New escrow/, "escrowNew"], [/^Create schedule/, "vesting"],
    [/^New proposal/, "dao"], [/^New plan|^Subscribe on behalf/, "subs"], [/^New transaction|^Deposit$|^Copy address/, "multisig"],
    [/^Execute$|^Confirm$|^Reject$/, "tx"], [/^Vote$/, "proposal"], [/^Release$/, "escrowDetail"], [/^Claim/, "tx"],
    [/^Review$/, "subs"], [/^View all$/, "activity"], [/^All sales$/, "royalties"],
    [/^Back$/, "escrow"], [/^Review escrow/, "tx"], [/^Dispute$/, "tx"], [/^Touch$/, "tx"],
    [/^Sign /, "tx"], [/^Update royalty|^settle_sale/, "tx"], [/^Charge now|^Run due charges/, "tx"],
    [/^Cancel$/, "escrow"], [/^Done$|^Try again$/, "dashboard"],
    [/^CLI$|soroban-forge events/, "dev"], [/^Manage$/, "settings"],
  ]);

  // Landing page: contract cards.
  if (here === P.landing) {
    const cards = { Escrow: "escrow", Vesting: "vesting", "Multi-sig Wallet": "multisig", "DAO Governance": "dao",
      Subscriptions: "subs", "Marketplace Royalties": "royalties" };
    document.querySelectorAll(".card").forEach((card) => {
      const h = card.querySelector("h2, h3");
      if (h && cards[text(h)]) go(card, P[cards[text(h)]]);
    });
  }
  // Connect wallet: every wallet option signs you in.
  if (here === P.connect) {
    document.querySelectorAll(".row").forEach((r) => {
      if (/Freighter|xBull|Albedo|LOBSTR|Ledger/.test(text(r)) && r.querySelector(".grow")) go(r, P.dashboard);
    });
  }
  // Escrow list: #40 is the open dispute, the rest open the detail view.
  if (here === P.escrow) {
    document.querySelectorAll("tbody tr").forEach((tr) => go(tr, /^#40\b/.test(text(tr)) ? P.dispute : P.escrowDetail));
  }
  if (here === P.dispute) {
    document.querySelectorAll(".modal-h .dim").forEach((x) => go(x, P.escrow));
  }
  // DAO: proposal cards open the vote view.
  if (here === P.dao) {
    document.querySelectorAll(".card").forEach((c) => {
      if (/^#\d+/.test(text(c))) go(c, P.proposal);
      else if (/Create a proposal/.test(text(c))) go(c, P.proposal);
    });
  }
  // Event tables (dashboard + activity): rows open the contract they mention.
  if (here === P.dashboard || here === P.activity) {
    const byContract = [[/Escrow #40|Escrow\s*#40/, "dispute"], [/Escrow/, "escrowDetail"], [/Multi-sig/, "multisig"],
      [/Proposal|DAO/, "proposal"], [/Subscription/, "subs"], [/Vesting/, "vesting"], [/Royalt/, "royalties"]];
    wire("tbody tr", byContract);
  }

  // Floating screen index (skipped when the screenshot script renders the page).
  if (!/HeadlessChrome/.test(navigator.userAgent) && here !== P.index) {
    const a = document.createElement("a");
    a.href = P.index;
    a.textContent = "All screens";
    a.className = "screens-fab";
    document.body.appendChild(a);
  }
  const css = document.createElement("style");
  css.textContent = `[data-nav]:hover{filter:brightness(1.15)} .side .nav[data-nav]:hover{background:var(--panel2)}
    .crumb-link{color:inherit;text-decoration:none} .crumb-link:hover{color:var(--text)}
    .screens-fab{position:fixed;right:20px;bottom:20px;z-index:99;padding:10px 16px;border-radius:999px;
      background:var(--ember);color:#1A0A02;font-weight:700;font-size:13px;text-decoration:none;
      box-shadow:0 8px 30px rgba(0,0,0,.5)}`;
  document.head.appendChild(css);
})();
