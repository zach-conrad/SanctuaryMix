import { useEffect } from "react";
import { Sidebar } from "./components/Sidebar";
import { TopBar } from "./components/TopBar";
import { useAutoMix } from "./store/automix";
import { useMixer } from "./store/mixer";
import { useRecordings } from "./store/recordings";
import { AssistView } from "./views/AssistView";
import { MixerView } from "./views/MixerView";
import { RecordingsView } from "./views/RecordingsView";
import { ScenesView } from "./views/ScenesView";
import { SettingsView } from "./views/SettingsView";
import { SetupView } from "./views/SetupView";
import { SignInView } from "./views/SignInView";

export default function App() {
  const view = useMixer((s) => s.view);
  const init = useMixer((s) => s.init);
  const needsSignIn = useMixer((s) => s.session !== null && !s.session.authenticated && !s.workingLocally);
  useEffect(() => {
    void init()
      .then(() => useAutoMix.getState().init())
      .then(() => useRecordings.getState().init());
  }, [init]);

  // Esc freezes auto-mix from anywhere: the quickest way for a person to take over.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const { status, freeze } = useAutoMix.getState();
      if (e.key === "Escape" && status?.engaged && !status.frozen) void freeze();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  if (needsSignIn) return <SignInView />;

  return (
    <div className="app">
      <TopBar />
      <div className="app-body">
        <Sidebar />
        <main>
          {view === "mixer" && <MixerView />}
          {view === "scenes" && <ScenesView />}
          {view === "recordings" && <RecordingsView />}
          {view === "assist" && <AssistView />}
          {view === "setup" && <SetupView />}
          {view === "settings" && <SettingsView />}
        </main>
      </div>
    </div>
  );
}
