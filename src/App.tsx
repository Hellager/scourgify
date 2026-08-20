import "./App.css";
import { lazy, Suspense, type ReactNode } from "react";
import { HashRouter, Route, Routes } from "react-router-dom";
import { AppShell } from "./components/AppShell";
import { Dashboard } from "./components/Dashboard";
import { Toaster } from "./components/ui/sonner";

const HistoryPage = lazy(() =>
  import("./components/HistoryPage").then((module) => ({
    default: module.HistoryPage,
  })),
);
const RulesPage = lazy(() =>
  import("./components/RulesPage").then((module) => ({
    default: module.RulesPage,
  })),
);
const SettingsPage = lazy(() =>
  import("./components/SettingsPage").then((module) => ({
    default: module.SettingsPage,
  })),
);
const AboutDialog = lazy(() =>
  import("./components/AboutDialog").then((module) => ({
    default: module.AboutDialog,
  })),
);
const GridMode = lazy(() =>
  import("./components/GridMode").then((module) => ({
    default: module.GridMode,
  })),
);

const MockTestPage = import.meta.env.DEV
  ? lazy(() =>
      import("./components/MockTestPage").then((module) => ({
        default: module.MockTestPage,
      })),
    )
  : null;

function RouteLoading() {
  return (
    <div
      aria-busy="true"
      className="grid min-h-32 place-items-center text-sm text-muted-foreground"
      role="status"
    >
      Loading...
    </div>
  );
}

function LazyRoute({ children }: { children: ReactNode }) {
  return <Suspense fallback={<RouteLoading />}>{children}</Suspense>;
}

function App() {
  return (
    <HashRouter>
      <Routes>
        <Route element={<AppShell dashboard={<Dashboard />} />}>
          <Route index element={null} />
          <Route
            path="/history"
            element={
              <LazyRoute>
                <HistoryPage />
              </LazyRoute>
            }
          />
          <Route
            path="/rules"
            element={
              <LazyRoute>
                <RulesPage />
              </LazyRoute>
            }
          />
          <Route
            path="/settings"
            element={
              <LazyRoute>
                <SettingsPage />
              </LazyRoute>
            }
          />
          {MockTestPage ? (
            <Route
              path="/mock"
              element={
                <Suspense fallback={<RouteLoading />}>
                  <MockTestPage />
                </Suspense>
              }
            />
          ) : null}
        </Route>
        <Route
          path="/about"
          element={
            <LazyRoute>
              <AboutDialog />
            </LazyRoute>
          }
        />
        <Route
          path="/grid"
          element={
            <LazyRoute>
              <GridMode />
            </LazyRoute>
          }
        />
      </Routes>
      <Toaster />
    </HashRouter>
  );
}

export default App;
