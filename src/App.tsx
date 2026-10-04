import { useEffect } from "react";
import { Sidebar } from "./components/Sidebar";
import { TopBar } from "./components/TopBar";
import { useMixer } from "./store/mixer";
import { AssistantView } from "./views/AssistantView";
import { MixView } from "./views/MixView";
import { SettingsView } from "./views/SettingsView";
import { SetupView } from "./views/SetupView";

export default function App() {
  const view = useMixer((s) => s.view);
  const init = useMixer((s) => s.init);
  useEffect(() => {
    void init();
  }, [init]);

  return (
    <div className="app">
      <Sidebar />
      <div className="content">
        <TopBar />
        <main>
          {view === "mix" && <MixView />}
          {view === "assistant" && <AssistantView />}
          {view === "setup" && <SetupView />}
          {view === "settings" && <SettingsView />}
        </main>
      </div>
    </div>
  );
}
