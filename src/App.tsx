import "./App.css";
import { HashRouter, Route, Routes } from "react-router-dom";
import { AboutDialog } from "./components/AboutDialog";
import { AppShell } from "./components/AppShell";
import { Dashboard } from "./components/Dashboard";
import { HistoryPage } from "./components/HistoryPage";
import { GridMode } from "./components/GridMode";
import { RulesPage } from "./components/RulesPage";
import { SettingsPage } from "./components/SettingsPage";
import { Toaster } from "./components/ui/sonner";

function App() {
  return (
    <HashRouter>
      <Routes>
        <Route element={<AppShell dashboard={<Dashboard />} />}>
          <Route index element={null} />
          <Route path="/history" element={<HistoryPage />} />
          <Route path="/rules" element={<RulesPage />} />
          <Route path="/settings" element={<SettingsPage />} />
        </Route>
        <Route path="/about" element={<AboutDialog />} />
        <Route path="/grid" element={<GridMode />} />
      </Routes>
      <Toaster />
    </HashRouter>
  );
}

export default App;
