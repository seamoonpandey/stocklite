<script lang="ts">
  import { onMount } from "svelte";
  import {
    archiveProduct, createBackup, createProduct, deleteProduct, getDashboardSummary, getDatabasePath,
    getStockReport, listMovements, listProducts, recordMovement, restoreBackup, updateProduct,
  } from "$lib/api";
  import { formatDate } from "$lib/format";
  import type { DashboardSummary, Movement, Product, StockReportRow } from "$lib/types";
  import "$lib/../app.css";

  type View = "Dashboard" | "Products" | "Stock" | "Transactions" | "Reports" | "Settings";
  type MovementType = "purchase" | "sale" | "adjustment";
  const navItems: { label: View; icon: string }[] = [
    { label: "Dashboard", icon: "⌂" }, { label: "Products", icon: "□" }, { label: "Stock", icon: "↕" },
    { label: "Transactions", icon: "≡" }, { label: "Reports", icon: "▤" },
  ];
  const today = new Date().toISOString().slice(0, 10);
  const monthStart = new Date(new Date().getFullYear(), new Date().getMonth(), 1).toISOString().slice(0, 10);

  let activeView: View = $state("Dashboard");
  let products: Product[] = $state([]);
  let allProducts: Product[] = $state([]);
  let movements: Movement[] = $state([]);
  let summary: DashboardSummary = $state({ activeProducts: 0, totalUnits: 0, lowStock: 0, recentMovements: [] });
  let report: StockReportRow[] = $state([]);
  let search = $state("");
  let movementSearch = $state("");
  let movementFilter = $state("");
  let reportFrom = $state(monthStart);
  let reportTo = $state(today);
  let showProductForm = $state(false);
  let showMovementForm = $state(false);
  let showEditForm = $state(false);
  let movementType: MovementType = $state("purchase");
  let editingProduct: Product | null = $state(null);
  let name = $state("");
  let unit = $state("pieces");
  let openingQuantity = $state(0);
  let reorderThreshold = $state<number | null>(null);
  let selectedProductId = $state(0);
  let quantity = $state(1);
  let occurredAt = $state(today);
  let note = $state("");
  let isSaving = $state(false);
  let isLoading = $state(false);
  let refreshSequence = 0;
  let error = $state("");
  let notice = $state("");
  let backupPath = $state("");
  let restorePath = $state("");
  let databasePath = $state("");

  let filteredProducts = $derived(products.filter((product) =>
    `${product.name} ${product.productCode}`.toLowerCase().includes(search.toLowerCase())));
  let filteredMovements = $derived(movements.filter((movement) =>
    (!movementFilter || movement.movementType === movementFilter) &&
    `${movement.productName} ${movement.note ?? ""}`.toLowerCase().includes(movementSearch.toLowerCase())));

  onMount(async () => { await refresh(); databasePath = await getDatabasePath(); });
  async function refresh() {
    const sequence = ++refreshSequence;
    isLoading = true;
    try {
      const [nextProducts, nextAllProducts, nextMovements, nextSummary] = await Promise.all([
        listProducts(), listProducts("", true), listMovements(), getDashboardSummary(),
      ]);
      if (sequence === refreshSequence) {
        products = nextProducts;
        allProducts = nextAllProducts;
        movements = nextMovements;
        summary = nextSummary;
      }
    } finally {
      isLoading = false;
    }
  }
  function clearMessages() { error = ""; notice = ""; }
  function openProductForm(product?: Product) {
    clearMessages();
    editingProduct = product ?? null;
    name = product?.name ?? ""; unit = product?.unit ?? "pieces";
    openingQuantity = 0; reorderThreshold = product?.reorderThreshold ?? null;
    if (product) showEditForm = true; else showProductForm = true;
  }
  async function saveProduct(event: SubmitEvent) {
    event.preventDefault(); clearMessages(); isSaving = true;
    try {
      if (editingProduct) {
        await updateProduct({ productId: editingProduct.id, name, unit, reorderThreshold });
        showEditForm = false;
      } else {
        await createProduct({ name, unit, openingQuantity, reorderThreshold });
        showProductForm = false;
      }
      await refresh();
    } catch (caught) { error = caught instanceof Error ? caught.message : String(caught); }
    finally { isSaving = false; }
  }
  async function archive(product: Product) {
    if (!window.confirm(`Archive ${product.name}? Its history will remain available.`)) return;
    clearMessages();
    try { await archiveProduct(product.id); notice = `${product.name} was archived.`; await refresh(); }
    catch (caught) { error = caught instanceof Error ? caught.message : String(caught); }
  }
  async function remove(product: Product) {
    if (!window.confirm(`Permanently delete ${product.name}? This is only allowed when it has no transaction history.`)) return;
    clearMessages();
    try { await deleteProduct(product.id); notice = `${product.name} was deleted.`; await refresh(); }
    catch (caught) { error = caught instanceof Error ? caught.message : String(caught); }
  }
  function openMovement(type: MovementType) {
    clearMessages(); movementType = type; selectedProductId = products[0]?.id ?? 0;
    quantity = type === "adjustment" ? 0 : 1; occurredAt = today; note = ""; showMovementForm = true;
  }
  async function saveMovement(event: SubmitEvent) {
    event.preventDefault(); clearMessages(); isSaving = true;
    try {
      await recordMovement(movementType, { productId: selectedProductId, quantity, occurredAt, note });
      showMovementForm = false; notice = `${movementType[0].toUpperCase()}${movementType.slice(1)} recorded.`; await refresh();
    } catch (caught) { error = caught instanceof Error ? caught.message : String(caught); }
    finally { isSaving = false; }
  }
  async function loadReport() {
    clearMessages();
    try { report = await getStockReport(reportFrom, reportTo); }
    catch (caught) { error = caught instanceof Error ? caught.message : String(caught); }
  }
  async function backup() {
    clearMessages();
    try { if (!backupPath.trim()) throw new Error("Enter a .db destination path."); await createBackup(backupPath.trim()); notice = "Backup created successfully."; }
    catch (caught) { error = caught instanceof Error ? caught.message : String(caught); }
  }
  async function restore() {
    clearMessages();
    if (!window.confirm("Restore this backup? Current data will be replaced. Make a backup first.")) return;
    try { if (!restorePath.trim()) throw new Error("Enter a .db backup path."); await restoreBackup(restorePath.trim()); notice = "Backup restored successfully."; await refresh(); }
    catch (caught) { error = caught instanceof Error ? caught.message : String(caught); }
  }
  function movementLabel(type: string) { return type === "purchase" ? "Purchase" : type === "sale" ? "Sale" : type === "opening" ? "Opening stock" : "Adjustment"; }
  function closeModal() { showProductForm = false; showEditForm = false; showMovementForm = false; error = ""; }
</script>

<svelte:head><title>StockLite · Inventory workspace</title></svelte:head>

<div class="app-shell">
  <aside class="sidebar">
    <div class="brand"><div class="brand-mark">S</div><div><strong>StockLite</strong><span>Inventory workspace</span></div></div>
    <nav><p class="nav-heading">WORKSPACE</p>
      {#each navItems as item}<button class:active={activeView === item.label} onclick={() => activeView = item.label}><span class="nav-icon">{item.icon}</span>{item.label}</button>{/each}
    </nav>
    <div class="sidebar-bottom"><div class="offline"><span></span><div><strong>Offline mode</strong><small>Data stays on this device</small></div></div>
      <button class:active={activeView === "Settings"} class="settings" onclick={() => activeView = "Settings"}>⚙ <span>Settings & data</span></button>
      <div class="profile"><div class="avatar">JD</div><div><strong>Local administrator</strong><small>Data stays on this device</small></div></div>
    </div>
  </aside>

  <main class="content">
    <header class="topbar"><div class="crumb">Workspace <span>/</span> <b>{activeView}</b></div><span class="today">Today, {formatDate(today)}</span></header>
    <section class="page">
      {#if error}<div class="alert error-banner">{error}<button onclick={() => error = ""}>×</button></div>{/if}
      {#if notice}<div class="alert success-banner">{notice}<button onclick={() => notice = ""}>×</button></div>{/if}

      {#if activeView === "Dashboard"}
        <div class="page-heading"><div><p class="eyebrow">OVERVIEW</p><h1>Good morning <span>✦</span></h1><p class="subtitle">Here’s what’s happening with your inventory today.</p></div><button class="primary" onclick={() => openProductForm()}>＋ Add product</button></div>
        <div class="stats"><div class="stat-card"><div class="stat-icon green">▣</div><div><span>Active products</span><strong>{summary.activeProducts}</strong><small class="muted">Currently tracked</small></div></div><div class="stat-card"><div class="stat-icon blue">◫</div><div><span>Total stock units</span><strong>{summary.totalUnits.toLocaleString()}</strong><small class="muted">Across all products</small></div></div><div class="stat-card"><div class="stat-icon amber">⌁</div><div><span>Low stock alerts</span><strong>{summary.lowStock}</strong><small class="warning">Needs attention</small></div></div></div>
        <div class="dashboard-grid"><section class="panel"><div class="panel-header"><div><h2>Recent activity</h2><p>Your latest inventory movements</p></div><button class="text-button" onclick={() => activeView = "Transactions"}>View all →</button></div>
          {#each summary.recentMovements as movement}<div class="activity-row"><div class="activity-icon {movement.movementType}">{movement.movementType === "sale" ? "↗" : movement.movementType === "purchase" ? "↙" : "↕"}</div><div class="activity-main"><strong>{movementLabel(movement.movementType)}</strong><span>{movement.productName}</span></div><div class:negative={movement.quantityDelta < 0} class="activity-quantity">{movement.quantityDelta > 0 ? "+" : ""}{movement.quantityDelta}</div><time>{formatDate(movement.occurredAt)}</time></div>{:else}<div class="empty">No activity yet.</div>{/each}
        </section><section class="panel quick"><div class="panel-header"><div><h2>Quick actions</h2><p>Keep your stock up to date</p></div></div>
          <button onclick={() => openMovement("purchase")}><span class="quick-icon purchase">↙</span><div><strong>Record a purchase</strong><small>Add incoming stock</small></div><span>→</span></button>
          <button onclick={() => openMovement("sale")}><span class="quick-icon sale">↗</span><div><strong>Record a sale</strong><small>Remove outgoing stock</small></div><span>→</span></button>
          <button onclick={() => openMovement("adjustment")}><span class="quick-icon adjustment">↕</span><div><strong>Make an adjustment</strong><small>Correct a discrepancy</small></div><span>→</span></button>
        </section></div>
      {:else if activeView === "Products"}
        <div class="page-heading"><div><p class="eyebrow">CATALOG</p><h1>Products</h1><p class="subtitle">Manage products, units, reorder thresholds, and history.</p></div><button class="primary" onclick={() => openProductForm()}>＋ Add product</button></div>
        <section class="panel table-panel"><div class="table-toolbar"><div class="search"><span>⌕</span><input placeholder="Search products..." bind:value={search} /></div><span class="result-count">{filteredProducts.length} active products</span></div>
          <table><thead><tr><th>PRODUCT</th><th>PRODUCT ID</th><th>UNIT</th><th>CURRENT STOCK</th><th>REORDER AT</th><th>STATUS</th><th></th></tr></thead><tbody>
            {#each filteredProducts as product}<tr><td><strong>{product.name}</strong></td><td class="code">{product.productCode}</td><td>{product.unit}</td><td><strong>{product.currentStock}</strong></td><td>{product.reorderThreshold ?? "—"}</td><td><span class:low={product.reorderThreshold !== null && product.currentStock <= product.reorderThreshold} class="status"><i></i>{product.reorderThreshold !== null && product.currentStock <= product.reorderThreshold ? "Low stock" : "In stock"}</span></td><td class="row-actions"><button onclick={() => openProductForm(product)}>Edit</button><button onclick={() => archive(product)}>Archive</button><button onclick={() => remove(product)}>Delete</button></td></tr>{:else}<tr><td colspan="7" class="empty">No products found.</td></tr>{/each}
          </tbody></table>{#if isLoading}<div class="table-loading">Refreshing inventory…</div>{/if}
        </section>
        {#if allProducts.some((product) => !product.isActive)}<p class="archived-note">{allProducts.filter((product) => !product.isActive).length} archived product(s) remain in transaction history.</p>{/if}
      {:else if activeView === "Stock"}
        <div class="page-heading"><div><p class="eyebrow">INVENTORY CONTROL</p><h1>Stock movements</h1><p class="subtitle">Every change is recorded in an append-only transaction history.</p></div></div>
        <div class="action-cards"><button onclick={() => openMovement("purchase")}><span class="quick-icon purchase">↙</span><strong>Purchase</strong><small>Receive incoming stock</small></button><button onclick={() => openMovement("sale")}><span class="quick-icon sale">↗</span><strong>Sale</strong><small>Record outgoing stock</small></button><button onclick={() => openMovement("adjustment")}><span class="quick-icon adjustment">↕</span><strong>Adjustment</strong><small>Use a negative value to reduce stock</small></button></div>
        <section class="panel table-panel stock-preview"><div class="panel-header"><div><h2>Current balances</h2><p>Select a quick action above to record a movement.</p></div></div><table><thead><tr><th>PRODUCT</th><th>UNIT</th><th>AVAILABLE</th><th>REORDER LEVEL</th></tr></thead><tbody>{#each products as product}<tr><td><strong>{product.name}</strong></td><td>{product.unit}</td><td>{product.currentStock}</td><td>{product.reorderThreshold ?? "—"}</td></tr>{/each}</tbody></table></section>
      {:else if activeView === "Transactions"}
        <div class="page-heading"><div><p class="eyebrow">AUDIT TRAIL</p><h1>Transactions</h1><p class="subtitle">A complete, append-only history of your inventory.</p></div></div>
        <section class="panel table-panel"><div class="table-toolbar"><div class="search"><span>⌕</span><input placeholder="Search transactions..." bind:value={movementSearch} /></div><select class="filter" bind:value={movementFilter}><option value="">All types</option><option value="opening">Opening</option><option value="purchase">Purchases</option><option value="sale">Sales</option><option value="adjustment">Adjustments</option></select></div>
          <table><thead><tr><th>DATE</th><th>PRODUCT</th><th>TYPE</th><th>QUANTITY CHANGE</th><th>RECORDED</th><th>NOTE</th></tr></thead><tbody>{#each filteredMovements as movement}<tr><td>{formatDate(movement.occurredAt)}</td><td><strong>{movement.productName}</strong></td><td><span class="type-pill {movement.movementType}">{movementLabel(movement.movementType)}</span></td><td class:negative={movement.quantityDelta < 0} class="delta">{movement.quantityDelta > 0 ? "+" : ""}{movement.quantityDelta}</td><td>{formatDate(movement.createdAt)}</td><td class="muted">{movement.note ?? "—"}</td></tr>{:else}<tr><td colspan="6" class="empty">No transactions found.</td></tr>{/each}</tbody></table>
        </section>
      {:else if activeView === "Reports"}
        <div class="page-heading"><div><p class="eyebrow">REPORTING</p><h1>Stock report</h1><p class="subtitle">Opening balance, activity, and closing balance for a selected period.</p></div><button class="primary" onclick={() => loadReport()}>Run report</button></div>
        <section class="panel report-controls"><label>From<input type="date" bind:value={reportFrom} /></label><label>To<input type="date" bind:value={reportTo} /></label></section>
        <section class="panel table-panel"><table><thead><tr><th>PRODUCT</th><th>OPENING</th><th>PURCHASES</th><th>SALES</th><th>ADJUSTMENTS</th><th>CLOSING</th></tr></thead><tbody>{#each report as row}<tr><td><strong>{row.productName}</strong><small class="muted">{row.unit}</small></td><td>{row.openingBalance}</td><td class="positive">+{row.purchases}</td><td class="negative">-{row.sales}</td><td>{row.adjustments > 0 ? "+" : ""}{row.adjustments}</td><td><strong>{row.closingBalance}</strong></td></tr>{:else}<tr><td colspan="6" class="empty">Choose a date range and run the report.</td></tr>{/each}</tbody></table></section>
      {:else}
        <div class="page-heading"><div><p class="eyebrow">DATA & RECOVERY</p><h1>Settings</h1><p class="subtitle">Protect your local inventory data and understand where it is stored.</p></div></div>
        <section class="settings-grid"><div class="panel settings-card"><h2>Database location</h2><p>Your database is stored locally and works without internet access.</p><code>{databasePath}</code><button class="secondary" onclick={() => getDatabasePath().then(value => databasePath = value)}>Refresh path</button></div>
          <div class="panel settings-card"><h2>Create backup</h2><p>Use a full .db destination path. Backups are consistent snapshots.</p><input placeholder="/Users/you/Documents/stocklite-backup.db" bind:value={backupPath} /><button class="primary" onclick={backup}>Create backup</button></div>
          <div class="panel settings-card"><h2>Restore backup</h2><p>Only restore a trusted StockLite .db file. Make a backup first.</p><input placeholder="/Users/you/Documents/stocklite-backup.db" bind:value={restorePath} /><button class="danger" onclick={restore}>Restore database</button></div>
        </section>
      {/if}
    </section>
  </main>
</div>

{#if showProductForm || showEditForm}
  <div class="modal-backdrop" role="presentation" onclick={(event) => event.target === event.currentTarget && closeModal()}><div class="modal"><button class="close" onclick={closeModal}>×</button><p class="eyebrow">{showEditForm ? "EDIT PRODUCT" : "NEW PRODUCT"}</p><h2>{showEditForm ? "Edit product" : "Add a product"}</h2><p class="modal-subtitle">{showEditForm ? "Product history remains unchanged." : "Create the product and its opening balance atomically."}</p><form onsubmit={saveProduct}><label>Product name<input required bind:value={name} placeholder="e.g. Arabica Coffee Beans" /></label><label>Unit<select bind:value={unit}><option>pieces</option><option>boxes</option><option>bags</option><option>packs</option><option>kg</option><option>litres</option></select></label>{#if !showEditForm}<label>Opening stock<input type="number" min="0" bind:value={openingQuantity} /></label>{/if}<label>Reorder threshold <span class="optional">Optional</span><input type="number" min="0" bind:value={reorderThreshold} placeholder="Alert below this balance" /></label><button class="primary wide" disabled={isSaving}>{isSaving ? "Saving..." : showEditForm ? "Save changes" : "Create product"}</button></form></div></div>
{:else if showMovementForm}
  <div class="modal-backdrop" role="presentation" onclick={(event) => event.target === event.currentTarget && closeModal()}><div class="modal"><button class="close" onclick={closeModal}>×</button><p class="eyebrow">{movementType.toUpperCase()}</p><h2>Record a {movementType}</h2><p class="modal-subtitle">This creates a permanent transaction record.</p><form onsubmit={saveMovement}><label>Product<select required bind:value={selectedProductId}>{#each products as product}<option value={product.id}>{product.name} · {product.currentStock} {product.unit}</option>{/each}</select></label><label>{movementType === "adjustment" ? "Signed quantity change" : "Quantity"}<input required type="number" min={movementType === "adjustment" ? undefined : 1} bind:value={quantity} /></label><label>Date<input required type="date" bind:value={occurredAt} /></label><label>Note <span class="optional">Optional</span><input bind:value={note} placeholder="Supplier, order, or reason" /></label><button class="primary wide" disabled={isSaving || products.length === 0}>{isSaving ? "Saving..." : "Save movement"}</button></form></div></div>
{/if}

<style>
  .app-shell{display:flex;min-height:100vh;background:#f7faf8}.sidebar{width:248px;background:#fff;border-right:1px solid #e7eee9;padding:28px 16px 18px;display:flex;flex-direction:column}.brand{display:flex;align-items:center;gap:11px;padding:0 12px 42px}.brand-mark{background:#2d8060;color:#fff;width:32px;height:32px;border-radius:10px;display:grid;place-items:center;font:800 18px Manrope}.brand strong{display:block;font:800 17px Manrope;letter-spacing:-.4px}.brand span{display:block;color:#91a097;font-size:10px;margin-top:1px}.nav-heading,.eyebrow{color:#8fa097;font-size:10px;font-weight:700;letter-spacing:1.4px}.nav-heading{padding:0 13px;margin:0 0 12px}.sidebar nav button{border:0;background:transparent;color:#7e8e84;width:100%;padding:11px 13px;border-radius:9px;text-align:left;font-size:13px;margin:2px 0}.sidebar nav button.active,.settings.active{background:#eaf6ef;color:#267653;font-weight:700}.nav-icon{display:inline-block;width:26px;font-size:17px}.sidebar-bottom{margin-top:auto}.offline{display:flex;gap:10px;background:#f7faf8;padding:12px;border-radius:9px;margin:0 4px 18px}.offline>span{background:#44a675;border:3px solid #d9f2e3;border-radius:50%;height:10px;width:10px;margin-top:3px}.offline strong,.offline small,.profile strong,.profile small{display:block}.offline strong{font-size:11px}.offline small,.profile small{color:#99a59e;font-size:10px;margin-top:2px}.settings{color:#87958d;width:100%;border:0;background:transparent;text-align:left;padding:11px 13px;font-size:12px;margin-bottom:14px}.profile{border-top:1px solid #edf1ee;padding:16px 4px 0;display:flex;align-items:center;gap:9px}.avatar{width:30px;height:30px;border-radius:50%;background:#e6e1d7;color:#836f4d;display:grid;place-items:center;font-size:10px;font-weight:700}.content{flex:1}.topbar{height:68px;border-bottom:1px solid #e7eee9;background:#fff;display:flex;align-items:center;justify-content:space-between;padding:0 42px}.crumb{color:#9daaa3;font-size:12px}.crumb span{margin:0 9px;color:#d1d9d4}.crumb b{color:#425248;font-weight:600}.today{font-size:11px;color:#89978f}.page{padding:36px 42px 60px;max-width:1400px}.page-heading{display:flex;justify-content:space-between;align-items:flex-end;margin-bottom:31px}.eyebrow{margin:0 0 8px}.page-heading h1{font:800 28px Manrope;margin:0;letter-spacing:-1.1px}.page-heading h1 span{font-size:19px;color:#d1a96c}.subtitle,.modal-subtitle{font-size:12px;color:#9aa69f;margin:8px 0 0}.primary,.secondary,.danger{border:0;border-radius:7px;padding:11px 16px;font-size:12px;font-weight:700}.primary{background:#2d8060;color:#fff;box-shadow:0 3px 8px #2d806021}.secondary{background:#edf5ef;color:#267653}.danger{background:#fff0ec;color:#bf6a54}.stats{display:grid;grid-template-columns:repeat(3,1fr);gap:18px;margin-bottom:24px}.stat-card,.panel{background:#fff;border:1px solid #e9efeb;border-radius:10px;box-shadow:0 3px 14px #153b2408}.stat-card{padding:20px;display:flex;gap:14px}.stat-card span{display:block;color:#88968e;font-size:11px;margin-bottom:7px}.stat-card strong{display:block;font:800 25px Manrope}.stat-card small{font-size:10px;display:block;margin-top:7px}.muted{color:#9ba7a0!important}.warning{color:#d08d53}.positive{color:#3d996d}.negative{color:#d5826a}.stat-icon,.quick-icon,.activity-icon{width:35px;height:35px;border-radius:9px;display:grid;place-items:center}.stat-icon.green{background:#e5f5eb;color:#328a63}.stat-icon.blue{background:#e9f1f8;color:#5681a6}.stat-icon.amber{background:#fdf1df;color:#c18b4b}.dashboard-grid{display:grid;grid-template-columns:1.35fr 1fr;gap:18px}.panel-header{display:flex;align-items:flex-start;justify-content:space-between;padding:22px 22px 14px}.panel h2{font:700 14px Manrope;margin:0}.panel-header p,.settings-card p{font-size:11px;color:#a0aba5;margin:5px 0 0}.text-button{border:0;background:transparent;color:#3b8b67;font-size:11px;font-weight:700}.activity-row{display:flex;align-items:center;gap:11px;margin:0 22px;padding:14px 0;border-top:1px solid #f0f3f1}.activity-icon.purchase,.quick-icon.purchase{background:#e7f4eb;color:#3f9b6b}.activity-icon.sale,.quick-icon.sale{background:#fff0e8;color:#db916d}.activity-icon.adjustment,.quick-icon.adjustment{background:#f0edfa;color:#8a78b8}.activity-main{flex:1}.activity-main strong,.activity-main span{display:block}.activity-main strong{font-size:11px}.activity-main span{font-size:10px;color:#a0aba5;margin-top:3px}.activity-quantity{font-size:12px;font-weight:700;color:#3b956a}.activity-row time{font-size:10px;color:#a8b0aa}.quick>button{display:flex;align-items:center;gap:11px;width:calc(100% - 44px);margin:0 22px;padding:14px 0;border:0;border-top:1px solid #f0f3f1;background:transparent;text-align:left}.quick>button>div{flex:1}.quick strong,.quick small{display:block}.quick strong{font-size:11px}.quick small{font-size:10px;color:#a0aba5;margin-top:3px}.empty{text-align:center;color:#a2ada6;font-size:12px;padding:34px}.table-panel{overflow:hidden}.table-toolbar{display:flex;align-items:center;gap:15px;padding:16px 20px;border-bottom:1px solid #edf1ee}.search{display:flex;align-items:center;gap:8px;border:1px solid #e4ebe6;border-radius:7px;padding:8px 10px;width:280px;color:#91a097}.search input{border:0;outline:0;color:#425248;font-size:11px;width:100%}.result-count{font-size:11px;color:#9aa69f}.filter{margin-left:auto;border:1px solid #dfe8e2;background:#fff;border-radius:6px;padding:8px 12px;color:#728178;font-size:11px}table{width:100%;border-collapse:collapse;text-align:left}th{color:#9ca9a1;font-size:9px;letter-spacing:.8px;font-weight:700;padding:13px 20px;background:#fbfcfb}td{padding:15px 20px;border-top:1px solid #f0f3f1;font-size:11px;color:#68776e}td strong{color:#314139;font-size:11px}td small{display:block}.code{font-family:monospace;color:#94a39a}.status{color:#39976a;font-size:10px}.status.low{color:#d17b5e}.status i{display:inline-block;background:#4eae7b;width:6px;height:6px;border-radius:50%;margin-right:4px}.status.low i{background:#d17b5e}.row-actions{white-space:nowrap}.row-actions button{border:0;background:transparent;color:#398b65;font-size:10px;margin-left:8px}.archived-note{color:#9aa69f;font-size:11px}.action-cards{display:grid;grid-template-columns:repeat(3,1fr);gap:18px;margin-bottom:24px}.action-cards button{background:#fff;border:1px solid #e9efeb;border-radius:10px;padding:22px;text-align:left;box-shadow:0 3px 14px #153b2408}.action-cards .quick-icon{margin-bottom:18px}.action-cards strong,.action-cards small{display:block}.action-cards strong{font-size:13px}.action-cards small{color:#99a69e;font-size:11px;margin-top:5px}.type-pill{border-radius:20px;padding:5px 9px;font-size:10px}.type-pill.purchase,.type-pill.opening{background:#e7f4eb;color:#398d63}.type-pill.sale{background:#fff0e8;color:#d17f61}.type-pill.adjustment{background:#f0edfa;color:#8371b2}.delta{font-weight:700;color:#3b956a}.report-controls{display:flex;gap:16px;padding:18px;margin-bottom:18px}.report-controls label{font-size:11px;color:#66766d;font-weight:700}.report-controls input,.settings-card input{display:block;margin-top:7px;padding:9px;border:1px solid #dfe8e2;border-radius:6px;font-size:12px}.settings-grid{display:grid;grid-template-columns:repeat(3,1fr);gap:18px}.settings-card{padding:22px;display:flex;flex-direction:column;align-items:flex-start;gap:14px}.settings-card h2{margin:0;font:700 14px Manrope}.settings-card code{display:block;background:#f5f8f6;color:#607168;font-size:10px;padding:10px;border-radius:6px;word-break:break-all;width:100%}.settings-card input{width:100%}.alert{padding:11px 14px;border-radius:7px;font-size:11px;margin-bottom:18px}.alert button{float:right;border:0;background:transparent;font-size:16px}.error-banner{background:#fff0ec;color:#bf6a54}.success-banner{background:#e7f4eb;color:#328a63}.modal-backdrop{position:fixed;inset:0;background:#18352645;display:grid;place-items:center;z-index:5}.modal{position:relative;background:#fff;width:400px;border-radius:12px;padding:30px;box-shadow:0 20px 60px #173d2780}.modal h2{font:800 22px Manrope;margin:0}.close{position:absolute;right:18px;top:15px;border:0;background:transparent;color:#9aa69f;font-size:22px}.modal form{margin-top:24px}.modal label{display:block;color:#66766d;font-size:11px;font-weight:700;margin-bottom:15px}.modal input,.modal select{display:block;width:100%;margin-top:7px;padding:10px;border:1px solid #dfe8e2;border-radius:6px;outline:0;color:#405047;font-size:12px;background:#fff}.optional{color:#aab4ae;font-weight:400;margin-left:4px}.wide{width:100%;margin-top:6px}
  .table-loading{padding:10px 20px;color:#7c8c83;font-size:10px;border-top:1px solid #f0f3f1}
  @media(max-width:1100px){.page{padding-left:28px;padding-right:28px}.topbar{padding:0 28px}.dashboard-grid,.settings-grid{grid-template-columns:1fr}.sidebar{width:220px}}
</style>
