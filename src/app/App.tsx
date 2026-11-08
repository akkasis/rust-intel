import { useEffect } from "react";
import { createHashRouter, Navigate, RouterProvider } from "react-router";
import { Toaster } from "sonner";
import { useAppInfo } from "./hooks";
import { FirstRun } from "./FirstRun";
import { AppShell } from "@/layout/AppShell";
import { ErrorState, LoadingBlock } from "@/components/ui";
import { DashboardPage } from "@/features/dashboard/DashboardPage";
import { SearchPage } from "@/features/search/SearchPage";
import { PlayersPage } from "@/features/players/PlayersPage";
import { ProfilePage } from "@/features/players/ProfilePage";
import { ActivityPage } from "@/features/activity/ActivityPage";
import { SquadsPage } from "@/features/squads/SquadsPage";
import { TrackerPage } from "@/features/tracker/TrackerPage";
import { BansPage } from "@/features/bans/BansPage";
import { ServersPage } from "@/features/servers/ServersPage";
import { IntegrationsPage } from "@/features/integrations/IntegrationsPage";
import { SettingsPage } from "@/features/settings/SettingsPage";
import { HelpPage } from "@/features/help/HelpPage";
import { BmHomePage } from "@/features/bm/BmHomePage";
import { BmPlayerPage } from "@/features/bm/BmPlayerPage";

const router = createHashRouter([
  {
    element: <AppShell />,
    errorElement: <ErrorState error="Страница не найдена" />,
    children: [
      { index: true, element: <DashboardPage /> },
      { path: "search", element: <SearchPage /> },
      { path: "players", element: <PlayersPage /> },
      { path: "players/:steamId", element: <ProfilePage /> },
      { path: "activity", element: <ActivityPage /> },
      { path: "squads", element: <SquadsPage /> },
      { path: "squads/:id", element: <SquadsPage /> },
      { path: "tracker", element: <TrackerPage /> },
      { path: "bans", element: <BansPage /> },
      { path: "servers", element: <ServersPage /> },
      { path: "integrations", element: <IntegrationsPage /> },
      { path: "settings", element: <SettingsPage /> },
      { path: "help", element: <HelpPage /> },
      { path: "bm", element: <BmHomePage /> },
      { path: "bm/:bmId", element: <BmPlayerPage /> },
      { path: "*", element: <Navigate to="/" replace /> },
    ],
  },
]);

function useThemeAndMotion() {
  const info = useAppInfo();
  const theme = info.data?.settings.theme ?? "dark";
  const motion = info.data?.settings.reduced_motion ?? "system";
  useEffect(() => {
    const apply = () => {
      const dark = theme === "dark" || (theme === "system" && window.matchMedia("(prefers-color-scheme: dark)").matches);
      document.documentElement.dataset.theme = dark ? "dark" : "light";
    };
    apply();
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    mq.addEventListener?.("change", apply);
    return () => mq.removeEventListener?.("change", apply);
  }, [theme]);
  useEffect(() => {
    if (motion === "on") document.documentElement.dataset.motion = "reduced";
    else delete document.documentElement.dataset.motion;
  }, [motion]);
  return theme;
}

export function App() {
  const info = useAppInfo();
  const theme = useThemeAndMotion();
  const toaster = <Toaster theme={theme === "light" ? "light" : "dark"} position="bottom-right" richColors closeButton />;
  if (info.isLoading)
    return (
      <div className="p-8">
        <LoadingBlock rows={6} />
      </div>
    );
  if (info.error) return <ErrorState error={info.error} retry={() => info.refetch()} />;
  if (info.data?.first_run)
    return (
      <>
        <FirstRun />
        {toaster}
      </>
    );
  return (
    <>
      <RouterProvider router={router} />
      {toaster}
    </>
  );
}
