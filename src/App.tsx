import "./App.css";
import { HashRouter, Route, Routes } from "react-router-dom";
import { AboutDialog } from "./components/AboutDialog";
import { Dashboard } from "./components/Dashboard";
import { Toaster } from "./components/ui/sonner";

function App() {
  return (
    <HashRouter>
      <Routes>
        <Route path="/" element={<Dashboard />} />
        <Route path="/about" element={<AboutDialog />} />
      </Routes>
      <Toaster />
    </HashRouter>
  );
}

export default App;
