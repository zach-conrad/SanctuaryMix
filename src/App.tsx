import { useEffect } from "react";
import { Sidebar } from "./components/Sidebar";
import { TopBar } from "./components/TopBar";
import { useMixer } from "./store/mixer";
import { AssistView } from "./views/AssistView";
import { MixerView } from "./views/MixerView";
import { ScenesView } from "./views/ScenesView";
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
      <TopBar />
      <div className="app-body">
        <Sidebar />
        <main>
          {view === "mixer" && <MixerView />}
          {view === "scenes" && <ScenesView />}
          {view === "assist" && <AssistView />}
          {view === "setup" && <SetupView />}
          {view === "settings" && <SettingsView />}
        </main>
      </div>
    </div>
  );
}
