import "./App.css";
import { HashRouter, Route, Routes } from "react-router-dom";
import { AboutDialog } from "./components/AboutDialog";
import { AppCommandPalette } from "./components/AppCommandPalette";
import { Dashboard } from "./components/Dashboard";
import { SettingsPage } from "./components/SettingsPage";
import { Toaster } from "./components/ui/sonner";

function App() {
  return (
    <HashRouter>
      <Routes>
        <Route path="/" element={<Dashboard />} />
        <Route path="/about" element={<AboutDialog />} />
        <Route path="/settings" element={<SettingsPage />} />
      </Routes>
      <AppCommandPalette />
      <Toaster />
    </HashRouter>
  );
}

export default App;
