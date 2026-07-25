import { Navigate, Route, Routes } from 'react-router-dom';
import { AppLayout } from './components/AppLayout';
import { CharactersPage } from './pages/CharactersPage';
import { CharacterEditorPage } from './pages/CharacterEditorPage';
import { TargetsPage } from './pages/TargetsPage';
import { PushPage } from './pages/PushPage';
import { HistoryPage } from './pages/HistoryPage';
import { HistoryDetailPage } from './pages/HistoryDetailPage';
import { SettingsPage } from './pages/SettingsPage';
import { Toaster } from './components/Toaster';
export default function App() {
  return (
    <>
      {' '}
      <Routes>
        {' '}
        <Route element={<AppLayout />}>
          {' '}
          <Route path="/" element={<Navigate to="/characters" replace />} />{' '}
          <Route path="/characters" element={<CharactersPage />} />{' '}
          <Route path="/characters/new" element={<CharacterEditorPage />} />{' '}
          <Route path="/characters/:id" element={<CharacterEditorPage />} />{' '}
          <Route path="/targets" element={<TargetsPage />} />{' '}
          <Route path="/push" element={<PushPage />} />{' '}
          <Route path="/history" element={<HistoryPage />} />{' '}
          <Route path="/history/:id" element={<HistoryDetailPage />} />{' '}
          <Route path="/settings" element={<SettingsPage />} />{' '}
          <Route path="*" element={<Navigate to="/characters" replace />} />{' '}
        </Route>{' '}
      </Routes>{' '}
      <Toaster />{' '}
    </>
  );
}
