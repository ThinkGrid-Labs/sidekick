import { Routes, Route, Navigate } from 'react-router-dom'
import Layout from './components/Layout'
import FlagList from './pages/FlagList'
import FlagEditor from './pages/FlagEditor'
import Settings from './pages/Settings'

export default function App() {
  return (
    <Layout>
      <Routes>
        <Route path="/" element={<FlagList />} />
        <Route path="/flags/new" element={<FlagEditor />} />
        <Route path="/flags/:key/edit" element={<FlagEditor />} />
        <Route path="/settings" element={<Settings />} />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
    </Layout>
  )
}
