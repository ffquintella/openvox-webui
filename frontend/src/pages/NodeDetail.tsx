import { useParams, Link } from 'react-router-dom';
import { useQuery } from '@tanstack/react-query';
import { ArrowLeft, Server, Clock, Folder } from 'lucide-react';
import { api } from '../services/api';

export default function NodeDetail() {
  const { certname } = useParams<{ certname: string }>();

  const { data: node, isLoading: nodeLoading } = useQuery({
    queryKey: ['node', certname],
    queryFn: () => api.getNode(certname!),
    enabled: !!certname,
  });

  const { data: facts, isLoading: factsLoading } = useQuery({
    queryKey: ['node-facts', certname],
    queryFn: () => api.getNodeFacts(certname!),
    enabled: !!certname,
  });

  if (nodeLoading || factsLoading) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary-600" />
      </div>
    );
  }

  if (!node) {
    return (
      <div className="text-center py-12">
        <p className="text-gray-500">Node not found</p>
        <Link to="/nodes" className="text-primary-600 hover:underline mt-4 block">
          Back to nodes
        </Link>
      </div>
    );
  }

  return (
    <div>
      {/* Header */}
      <div className="mb-8">
        <Link
          to="/nodes"
          className="flex items-center text-gray-500 hover:text-gray-700 mb-4"
        >
          <ArrowLeft className="w-4 h-4 mr-2" />
          Back to nodes
        </Link>
        <div className="flex items-center">
          <div className="p-3 bg-primary-50 rounded-lg mr-4">
            <Server className="w-8 h-8 text-primary-600" />
          </div>
          <div>
            <h1 className="text-2xl font-bold text-gray-900">{node.certname}</h1>
            <p className="text-gray-500">
              {node.catalog_environment || 'No environment'}
            </p>
          </div>
        </div>
      </div>

      {/* Info Cards */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-6 mb-8">
        <div className="card">
          <div className="flex items-center">
            <Clock className="w-5 h-5 text-gray-400 mr-3" />
            <div>
              <p className="text-sm text-gray-500">Last Report</p>
              <p className="font-medium">
                {node.report_timestamp
                  ? new Date(node.report_timestamp).toLocaleString()
                  : 'Never'}
              </p>
            </div>
          </div>
        </div>

        <div className="card">
          <div className="flex items-center">
            <Folder className="w-5 h-5 text-gray-400 mr-3" />
            <div>
              <p className="text-sm text-gray-500">Environment</p>
              <p className="font-medium">
                {node.catalog_environment || 'Unknown'}
              </p>
            </div>
          </div>
        </div>

        <div className="card">
          <div className="flex items-center">
            <Server className="w-5 h-5 text-gray-400 mr-3" />
            <div>
              <p className="text-sm text-gray-500">Status</p>
              <p className="font-medium capitalize">
                {node.latest_report_status || 'Unknown'}
              </p>
            </div>
          </div>
        </div>
      </div>

      {/* Facts */}
      <div className="card">
        <h2 className="text-lg font-semibold text-gray-900 mb-4">Facts</h2>
        <div className="overflow-x-auto">
          <pre className="text-sm text-gray-700 bg-gray-50 p-4 rounded-lg overflow-auto max-h-96">
            {JSON.stringify(facts || {}, null, 2)}
          </pre>
        </div>
      </div>
    </div>
  );
}
