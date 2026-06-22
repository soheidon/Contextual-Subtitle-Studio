import { useEffect } from "react";
import { BrowserRouter, Routes, Route } from "react-router-dom";
import Sidebar from "./components/layout/Sidebar";
import CharacterDictBuilder from "./components/scraper/CharacterDictBuilder";
import AppLogPanel from "./components/layout/AppLogPanel";
import DramaCharacterScraper from "./components/scraper/DramaCharacterScraper";
import SrtPage from "./components/srt/SrtPage";
import CharacterDict from "./components/dictionary/CharacterDict";
import GlossaryTable from "./components/dictionary/GlossaryTable";
import ProviderConfig from "./components/llm/ProviderConfig";
import TranslatePanel from "./components/translation/TranslatePanel";
import IssueList from "./components/review/IssueList";
import SettingsPanel from "./components/settings/SettingsPanel";
import { useLlmStore } from "./stores/useLlmStore";
import { useAppLogStore } from "./stores/useAppLogStore";
import "./App.css";

function Init() {
  const refresh = useLlmStore((s) => s.refresh);
  useEffect(() => {
    refresh();
  }, [refresh]);

  // Start Rust→Frontend log bridge once on mount
  useEffect(() => {
    const unlistenPromise = useAppLogStore.getState().startRustListener();
    return () => { unlistenPromise.then((fn) => fn()); };
  }, []);

  return null;
}

function App() {
  return (
    <BrowserRouter>
      <Init />
      <div className="app-container">
        <div className="app-body">
          <Sidebar />
          <main className="main-content">
            <Routes>
              <Route path="/scrape" element={<DramaCharacterScraper />} />
              <Route path="/" element={<CharacterDictBuilder />} />
              <Route path="/srt" element={<SrtPage />} />
              <Route
                path="/dictionaries"
                element={
                  <div>
                    <CharacterDict />
                    <div style={{ marginTop: 16 }}><GlossaryTable /></div>
                  </div>
                }
              />
              <Route path="/llm" element={<ProviderConfig />} />
              <Route path="/translate" element={<TranslatePanel />} />
              <Route path="/review" element={<IssueList />} />
              <Route path="/settings" element={<SettingsPanel />} />
            </Routes>
          </main>
        </div>
      <AppLogPanel />
      </div>
    </BrowserRouter>
  );
}

export default App;
