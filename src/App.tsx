import "./App.css";
import { HashRouter, Link, Route, Routes } from "react-router-dom";
import { AboutDialog } from "./components/AboutDialog";

function App() {
  return (
    <HashRouter>
      <Routes>
        <Route path="/" element={<DashboardPlaceholder />} />
        <Route path="/about" element={<AboutDialog />} />
      </Routes>
    </HashRouter>
  );
}

function DashboardPlaceholder() {
  return (
    <main className="min-h-screen bg-background text-foreground">
      <header className="flex h-14 items-center justify-between border-b px-6">
        <div>
          <h1 className="text-base font-semibold">Scourgify</h1>
          <p className="text-xs text-muted-foreground">Dashboard</p>
        </div>
        <Link className="text-sm text-muted-foreground hover:text-foreground" to="/about">
          About
        </Link>
      </header>
      <section className="grid gap-4 p-6 sm:grid-cols-2 lg:grid-cols-4">
        {["Recent Files", "Frequent Folders", "All", "Privacy"].map((label) => (
          <div className="rounded-md border bg-card p-4 text-card-foreground" key={label}>
            <p className="text-sm text-muted-foreground">{label}</p>
            <p className="mt-3 text-2xl font-semibold">--</p>
          </div>
        ))}
      </section>
    </main>
  );
}

export default App;
