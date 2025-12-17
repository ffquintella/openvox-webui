import { Routes, Route } from 'react-router-dom';
import Layout from './components/Layout';
import Dashboard from './pages/Dashboard';
import Nodes from './pages/Nodes';
import NodeDetail from './pages/NodeDetail';
import Groups from './pages/Groups';
import Reports from './pages/Reports';
import Facts from './pages/Facts';
import Settings from './pages/Settings';
import Roles from './pages/Roles';
import Users from './pages/Users';
import Permissions from './pages/Permissions';

function App() {
  return (
    <Layout>
      <Routes>
        <Route path="/" element={<Dashboard />} />
        <Route path="/nodes" element={<Nodes />} />
        <Route path="/nodes/:certname" element={<NodeDetail />} />
        <Route path="/groups" element={<Groups />} />
        <Route path="/reports" element={<Reports />} />
        <Route path="/facts" element={<Facts />} />
        <Route path="/roles" element={<Roles />} />
        <Route path="/users" element={<Users />} />
        <Route path="/permissions" element={<Permissions />} />
        <Route path="/settings" element={<Settings />} />
      </Routes>
    </Layout>
  );
}

export default App;
