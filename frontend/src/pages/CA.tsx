import { useState } from 'react';
import {
  Shield,
  FileCheck,
  FileClock,
  AlertTriangle,
  CheckCircle2,
  XCircle,
  RefreshCw,
  Clock,
  Key,
  FileX,
  Loader2,
} from 'lucide-react';
import clsx from 'clsx';
import {
  useCAStatus,
  useCertificateRequests,
  useCertificates,
  useSignCertificate,
  useRejectCertificate,
  useRevokeCertificate,
} from '../hooks/useCA';
import type { CertificateRequest, Certificate } from '../types';

type TabType = 'overview' | 'requests' | 'certificates';

export default function CA() {
  const [activeTab, setActiveTab] = useState<TabType>('overview');
  const [confirmAction, setConfirmAction] = useState<{
    type: 'sign' | 'reject' | 'revoke';
    certname: string;
  } | null>(null);

  const { data: status, isLoading: statusLoading, error: statusError } = useCAStatus();
  const { data: requests = [], isLoading: requestsLoading } = useCertificateRequests();
  const { data: certificates = [], isLoading: certificatesLoading } = useCertificates();

  const signMutation = useSignCertificate();
  const rejectMutation = useRejectCertificate();
  const revokeMutation = useRevokeCertificate();

  const handleConfirmAction = async () => {
    if (!confirmAction) return;

    try {
      if (confirmAction.type === 'sign') {
        await signMutation.mutateAsync({ certname: confirmAction.certname });
      } else if (confirmAction.type === 'reject') {
        await rejectMutation.mutateAsync(confirmAction.certname);
      } else if (confirmAction.type === 'revoke') {
        await revokeMutation.mutateAsync(confirmAction.certname);
      }
      setConfirmAction(null);
    } catch {
      // Error is handled by mutation
    }
  };

  const isActionPending =
    signMutation.isPending || rejectMutation.isPending || revokeMutation.isPending;

  // Calculate certificates expiring soon (within 30 days)
  const expiringCerts = certificates.filter((cert) => {
    if (!cert.not_after) return false;
    const expiresAt = new Date(cert.not_after);
    const daysUntilExpiry = (expiresAt.getTime() - Date.now()) / (1000 * 60 * 60 * 24);
    return daysUntilExpiry > 0 && daysUntilExpiry <= 30;
  });

  // Calculate CA expiration warning
  const caExpirationWarning = (() => {
    if (!status?.ca_expires_at) return null;
    const expiresAt = new Date(status.ca_expires_at);
    const daysUntilExpiry = (expiresAt.getTime() - Date.now()) / (1000 * 60 * 60 * 24);
    if (daysUntilExpiry <= 0) return { level: 'critical', days: 0, message: 'CA certificate has expired!' };
    if (daysUntilExpiry <= 30) return { level: 'warning', days: Math.floor(daysUntilExpiry), message: `CA certificate expires in ${Math.floor(daysUntilExpiry)} days` };
    if (daysUntilExpiry <= 90) return { level: 'info', days: Math.floor(daysUntilExpiry), message: `CA certificate expires in ${Math.floor(daysUntilExpiry)} days` };
    return null;
  })();

  const tabs = [
    { id: 'overview' as const, name: 'Overview', icon: Shield },
    {
      id: 'requests' as const,
      name: 'Pending Requests',
      icon: FileClock,
      badge: requests.length > 0 ? requests.length : undefined,
    },
    { id: 'certificates' as const, name: 'Certificates', icon: FileCheck },
  ];

  return (
    <div className="space-y-6">
      {/* Header */}
      <div>
        <h1 className="text-2xl font-bold text-gray-900">Certificate Authority</h1>
        <p className="mt-1 text-sm text-gray-500">
          Manage Puppet CA certificates and signing requests
        </p>
      </div>

      {/* CA Not Available Warning */}
      {statusError && (
        <div className="bg-yellow-50 border border-yellow-200 rounded-lg p-4">
          <div className="flex items-center gap-3">
            <AlertTriangle className="w-5 h-5 text-yellow-600" />
            <div>
              <p className="font-medium text-yellow-800">Puppet CA Not Available</p>
              <p className="text-sm text-yellow-700 mt-1">
                The Puppet CA service is not configured or not reachable. Check your configuration.
              </p>
            </div>
          </div>
        </div>
      )}

      {/* CA Expiration Warning */}
      {caExpirationWarning && (
        <div
          className={clsx(
            'border rounded-lg p-4',
            caExpirationWarning.level === 'critical' && 'bg-red-50 border-red-200',
            caExpirationWarning.level === 'warning' && 'bg-yellow-50 border-yellow-200',
            caExpirationWarning.level === 'info' && 'bg-blue-50 border-blue-200'
          )}
        >
          <div className="flex items-center gap-3">
            <AlertTriangle
              className={clsx(
                'w-5 h-5',
                caExpirationWarning.level === 'critical' && 'text-red-600',
                caExpirationWarning.level === 'warning' && 'text-yellow-600',
                caExpirationWarning.level === 'info' && 'text-blue-600'
              )}
            />
            <div>
              <p
                className={clsx(
                  'font-medium',
                  caExpirationWarning.level === 'critical' && 'text-red-800',
                  caExpirationWarning.level === 'warning' && 'text-yellow-800',
                  caExpirationWarning.level === 'info' && 'text-blue-800'
                )}
              >
                {caExpirationWarning.message}
              </p>
            </div>
          </div>
        </div>
      )}

      {/* Tabs */}
      <div className="border-b border-gray-200">
        <nav className="-mb-px flex space-x-8">
          {tabs.map((tab) => (
            <button
              key={tab.id}
              onClick={() => setActiveTab(tab.id)}
              className={clsx(
                'flex items-center gap-2 py-4 px-1 border-b-2 font-medium text-sm',
                activeTab === tab.id
                  ? 'border-primary-500 text-primary-600'
                  : 'border-transparent text-gray-500 hover:text-gray-700 hover:border-gray-300'
              )}
            >
              <tab.icon className="w-5 h-5" />
              {tab.name}
              {tab.badge && (
                <span className="ml-2 bg-yellow-100 text-yellow-800 text-xs font-medium px-2 py-0.5 rounded-full">
                  {tab.badge}
                </span>
              )}
            </button>
          ))}
        </nav>
      </div>

      {/* Tab Content */}
      {activeTab === 'overview' && (
        <OverviewTab
          status={status}
          statusLoading={statusLoading}
          requests={requests}
          certificates={certificates}
          expiringCerts={expiringCerts}
          onViewRequests={() => setActiveTab('requests')}
          onViewCertificates={() => setActiveTab('certificates')}
        />
      )}

      {activeTab === 'requests' && (
        <RequestsTab
          requests={requests}
          isLoading={requestsLoading}
          onSign={(certname) => setConfirmAction({ type: 'sign', certname })}
          onReject={(certname) => setConfirmAction({ type: 'reject', certname })}
        />
      )}

      {activeTab === 'certificates' && (
        <CertificatesTab
          certificates={certificates}
          isLoading={certificatesLoading}
          onRevoke={(certname) => setConfirmAction({ type: 'revoke', certname })}
        />
      )}

      {/* Confirmation Modal */}
      {confirmAction && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
          <div className="bg-white rounded-lg shadow-xl max-w-md w-full mx-4 p-6">
            <h3 className="text-lg font-semibold text-gray-900">
              {confirmAction.type === 'sign' && 'Sign Certificate Request'}
              {confirmAction.type === 'reject' && 'Reject Certificate Request'}
              {confirmAction.type === 'revoke' && 'Revoke Certificate'}
            </h3>
            <p className="mt-2 text-sm text-gray-600">
              {confirmAction.type === 'sign' && (
                <>
                  Are you sure you want to sign the certificate request for{' '}
                  <span className="font-mono font-medium">{confirmAction.certname}</span>?
                </>
              )}
              {confirmAction.type === 'reject' && (
                <>
                  Are you sure you want to reject the certificate request for{' '}
                  <span className="font-mono font-medium">{confirmAction.certname}</span>? This
                  action cannot be undone.
                </>
              )}
              {confirmAction.type === 'revoke' && (
                <>
                  Are you sure you want to revoke the certificate for{' '}
                  <span className="font-mono font-medium">{confirmAction.certname}</span>? This
                  will immediately invalidate the certificate.
                </>
              )}
            </p>
            <div className="mt-6 flex justify-end gap-3">
              <button
                onClick={() => setConfirmAction(null)}
                disabled={isActionPending}
                className="px-4 py-2 text-sm font-medium text-gray-700 bg-white border border-gray-300 rounded-md hover:bg-gray-50"
              >
                Cancel
              </button>
              <button
                onClick={handleConfirmAction}
                disabled={isActionPending}
                className={clsx(
                  'px-4 py-2 text-sm font-medium text-white rounded-md flex items-center gap-2',
                  confirmAction.type === 'sign' && 'bg-green-600 hover:bg-green-700',
                  confirmAction.type === 'reject' && 'bg-yellow-600 hover:bg-yellow-700',
                  confirmAction.type === 'revoke' && 'bg-red-600 hover:bg-red-700'
                )}
              >
                {isActionPending && <Loader2 className="w-4 h-4 animate-spin" />}
                {confirmAction.type === 'sign' && 'Sign'}
                {confirmAction.type === 'reject' && 'Reject'}
                {confirmAction.type === 'revoke' && 'Revoke'}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

// Overview Tab Component
function OverviewTab({
  status,
  statusLoading,
  requests,
  certificates,
  expiringCerts,
  onViewRequests,
  onViewCertificates,
}: {
  status: ReturnType<typeof useCAStatus>['data'];
  statusLoading: boolean;
  requests: CertificateRequest[];
  certificates: Certificate[];
  expiringCerts: Certificate[];
  onViewRequests: () => void;
  onViewCertificates: () => void;
}) {
  return (
    <div className="space-y-6">
      {/* Status Cards */}
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
        {/* CA Status */}
        <div className="bg-white rounded-lg border border-gray-200 p-6">
          <div className="flex items-center justify-between">
            <div>
              <p className="text-sm font-medium text-gray-500">CA Status</p>
              {statusLoading ? (
                <div className="mt-1 h-8 w-24 bg-gray-200 animate-pulse rounded" />
              ) : (
                <p className="mt-1 text-2xl font-semibold">
                  {status?.available ? (
                    <span className="text-green-600 flex items-center gap-2">
                      <CheckCircle2 className="w-6 h-6" /> Active
                    </span>
                  ) : (
                    <span className="text-red-600 flex items-center gap-2">
                      <XCircle className="w-6 h-6" /> Unavailable
                    </span>
                  )}
                </p>
              )}
            </div>
            <Shield className="w-10 h-10 text-gray-300" />
          </div>
        </div>

        {/* Pending Requests */}
        <div
          className="bg-white rounded-lg border border-gray-200 p-6 cursor-pointer hover:border-primary-300 transition-colors"
          onClick={onViewRequests}
        >
          <div className="flex items-center justify-between">
            <div>
              <p className="text-sm font-medium text-gray-500">Pending Requests</p>
              {statusLoading ? (
                <div className="mt-1 h-8 w-16 bg-gray-200 animate-pulse rounded" />
              ) : (
                <p className="mt-1 text-2xl font-semibold text-yellow-600">
                  {status?.pending_requests ?? requests.length}
                </p>
              )}
            </div>
            <FileClock className="w-10 h-10 text-yellow-300" />
          </div>
          {requests.length > 0 && (
            <p className="mt-2 text-sm text-yellow-600">Click to review</p>
          )}
        </div>

        {/* Signed Certificates */}
        <div
          className="bg-white rounded-lg border border-gray-200 p-6 cursor-pointer hover:border-primary-300 transition-colors"
          onClick={onViewCertificates}
        >
          <div className="flex items-center justify-between">
            <div>
              <p className="text-sm font-medium text-gray-500">Signed Certificates</p>
              {statusLoading ? (
                <div className="mt-1 h-8 w-16 bg-gray-200 animate-pulse rounded" />
              ) : (
                <p className="mt-1 text-2xl font-semibold text-green-600">
                  {status?.signed_certificates ?? certificates.length}
                </p>
              )}
            </div>
            <FileCheck className="w-10 h-10 text-green-300" />
          </div>
        </div>

        {/* Expiring Soon */}
        <div className="bg-white rounded-lg border border-gray-200 p-6">
          <div className="flex items-center justify-between">
            <div>
              <p className="text-sm font-medium text-gray-500">Expiring Soon</p>
              <p
                className={clsx(
                  'mt-1 text-2xl font-semibold',
                  expiringCerts.length > 0 ? 'text-orange-600' : 'text-gray-600'
                )}
              >
                {expiringCerts.length}
              </p>
            </div>
            <Clock className="w-10 h-10 text-orange-300" />
          </div>
          {expiringCerts.length > 0 && (
            <p className="mt-2 text-sm text-orange-600">Within 30 days</p>
          )}
        </div>
      </div>

      {/* CA Certificate Info */}
      {status?.available && (
        <div className="bg-white rounded-lg border border-gray-200 p-6">
          <h3 className="text-lg font-medium text-gray-900 mb-4 flex items-center gap-2">
            <Key className="w-5 h-5" />
            CA Certificate
          </h3>
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            <div>
              <p className="text-sm font-medium text-gray-500">Fingerprint (SHA256)</p>
              <p className="mt-1 font-mono text-sm text-gray-900 break-all">
                {status.ca_fingerprint || 'Not available'}
              </p>
            </div>
            <div>
              <p className="text-sm font-medium text-gray-500">Expires</p>
              <p className="mt-1 text-sm text-gray-900">
                {status.ca_expires_at
                  ? new Date(status.ca_expires_at).toLocaleDateString(undefined, {
                      year: 'numeric',
                      month: 'long',
                      day: 'numeric',
                    })
                  : 'Not available'}
              </p>
            </div>
          </div>
        </div>
      )}

      {/* Quick Actions */}
      <div className="bg-white rounded-lg border border-gray-200 p-6">
        <h3 className="text-lg font-medium text-gray-900 mb-4">Quick Actions</h3>
        <div className="flex flex-wrap gap-3">
          <button
            onClick={onViewRequests}
            disabled={requests.length === 0}
            className={clsx(
              'flex items-center gap-2 px-4 py-2 text-sm font-medium rounded-md',
              requests.length > 0
                ? 'bg-yellow-100 text-yellow-800 hover:bg-yellow-200'
                : 'bg-gray-100 text-gray-400 cursor-not-allowed'
            )}
          >
            <FileClock className="w-4 h-4" />
            Review Pending ({requests.length})
          </button>
          <button
            onClick={onViewCertificates}
            className="flex items-center gap-2 px-4 py-2 text-sm font-medium bg-gray-100 text-gray-800 rounded-md hover:bg-gray-200"
          >
            <FileCheck className="w-4 h-4" />
            View All Certificates
          </button>
          <button
            onClick={() => window.location.reload()}
            className="flex items-center gap-2 px-4 py-2 text-sm font-medium bg-gray-100 text-gray-800 rounded-md hover:bg-gray-200"
          >
            <RefreshCw className="w-4 h-4" />
            Refresh Status
          </button>
        </div>
      </div>
    </div>
  );
}

// Requests Tab Component
function RequestsTab({
  requests,
  isLoading,
  onSign,
  onReject,
}: {
  requests: CertificateRequest[];
  isLoading: boolean;
  onSign: (certname: string) => void;
  onReject: (certname: string) => void;
}) {
  if (isLoading) {
    return (
      <div className="flex items-center justify-center py-12">
        <Loader2 className="w-8 h-8 text-primary-600 animate-spin" />
      </div>
    );
  }

  if (requests.length === 0) {
    return (
      <div className="bg-white rounded-lg border border-gray-200 p-12 text-center">
        <FileClock className="w-12 h-12 text-gray-300 mx-auto" />
        <h3 className="mt-4 text-lg font-medium text-gray-900">No Pending Requests</h3>
        <p className="mt-2 text-sm text-gray-500">
          There are no certificate signing requests waiting for approval.
        </p>
      </div>
    );
  }

  return (
    <div className="bg-white rounded-lg border border-gray-200 overflow-hidden">
      <table className="min-w-full divide-y divide-gray-200">
        <thead className="bg-gray-50">
          <tr>
            <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">
              Certname
            </th>
            <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">
              Requested At
            </th>
            <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">
              Fingerprint
            </th>
            <th className="px-6 py-3 text-right text-xs font-medium text-gray-500 uppercase tracking-wider">
              Actions
            </th>
          </tr>
        </thead>
        <tbody className="bg-white divide-y divide-gray-200">
          {requests.map((request) => (
            <tr key={request.certname} className="hover:bg-gray-50">
              <td className="px-6 py-4 whitespace-nowrap">
                <span className="font-mono text-sm text-gray-900">{request.certname}</span>
              </td>
              <td className="px-6 py-4 whitespace-nowrap text-sm text-gray-500">
                {new Date(request.requested_at).toLocaleString()}
              </td>
              <td className="px-6 py-4">
                <span className="font-mono text-xs text-gray-500 break-all">
                  {request.fingerprint.substring(0, 32)}...
                </span>
              </td>
              <td className="px-6 py-4 whitespace-nowrap text-right">
                <div className="flex items-center justify-end gap-2">
                  <button
                    onClick={() => onSign(request.certname)}
                    className="inline-flex items-center gap-1 px-3 py-1.5 text-sm font-medium text-green-700 bg-green-100 rounded-md hover:bg-green-200"
                  >
                    <CheckCircle2 className="w-4 h-4" />
                    Sign
                  </button>
                  <button
                    onClick={() => onReject(request.certname)}
                    className="inline-flex items-center gap-1 px-3 py-1.5 text-sm font-medium text-red-700 bg-red-100 rounded-md hover:bg-red-200"
                  >
                    <XCircle className="w-4 h-4" />
                    Reject
                  </button>
                </div>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

// Certificates Tab Component
function CertificatesTab({
  certificates,
  isLoading,
  onRevoke,
}: {
  certificates: Certificate[];
  isLoading: boolean;
  onRevoke: (certname: string) => void;
}) {
  const [search, setSearch] = useState('');
  const [sortBy, setSortBy] = useState<'certname' | 'expires'>('certname');

  const filteredCerts = certificates
    .filter((cert) => cert.certname.toLowerCase().includes(search.toLowerCase()))
    .sort((a, b) => {
      if (sortBy === 'certname') {
        return a.certname.localeCompare(b.certname);
      } else {
        return new Date(a.not_after).getTime() - new Date(b.not_after).getTime();
      }
    });

  const getDaysUntilExpiry = (notAfter: string) => {
    const days = (new Date(notAfter).getTime() - Date.now()) / (1000 * 60 * 60 * 24);
    return Math.floor(days);
  };

  if (isLoading) {
    return (
      <div className="flex items-center justify-center py-12">
        <Loader2 className="w-8 h-8 text-primary-600 animate-spin" />
      </div>
    );
  }

  if (certificates.length === 0) {
    return (
      <div className="bg-white rounded-lg border border-gray-200 p-12 text-center">
        <FileCheck className="w-12 h-12 text-gray-300 mx-auto" />
        <h3 className="mt-4 text-lg font-medium text-gray-900">No Certificates</h3>
        <p className="mt-2 text-sm text-gray-500">
          There are no signed certificates in the CA.
        </p>
      </div>
    );
  }

  return (
    <div className="space-y-4">
      {/* Search and Sort */}
      <div className="flex items-center gap-4">
        <input
          type="text"
          placeholder="Search by certname..."
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          className="flex-1 px-4 py-2 border border-gray-300 rounded-md focus:ring-primary-500 focus:border-primary-500"
        />
        <select
          value={sortBy}
          onChange={(e) => setSortBy(e.target.value as 'certname' | 'expires')}
          className="px-4 py-2 border border-gray-300 rounded-md focus:ring-primary-500 focus:border-primary-500"
        >
          <option value="certname">Sort by Name</option>
          <option value="expires">Sort by Expiration</option>
        </select>
      </div>

      {/* Certificates Table */}
      <div className="bg-white rounded-lg border border-gray-200 overflow-hidden">
        <table className="min-w-full divide-y divide-gray-200">
          <thead className="bg-gray-50">
            <tr>
              <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">
                Certname
              </th>
              <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">
                Serial
              </th>
              <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">
                Expires
              </th>
              <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">
                Status
              </th>
              <th className="px-6 py-3 text-right text-xs font-medium text-gray-500 uppercase tracking-wider">
                Actions
              </th>
            </tr>
          </thead>
          <tbody className="bg-white divide-y divide-gray-200">
            {filteredCerts.map((cert) => {
              const daysUntilExpiry = getDaysUntilExpiry(cert.not_after);
              const isExpired = daysUntilExpiry < 0;
              const isExpiringSoon = daysUntilExpiry >= 0 && daysUntilExpiry <= 30;

              return (
                <tr key={cert.certname} className="hover:bg-gray-50">
                  <td className="px-6 py-4 whitespace-nowrap">
                    <span className="font-mono text-sm text-gray-900">{cert.certname}</span>
                  </td>
                  <td className="px-6 py-4 whitespace-nowrap text-sm text-gray-500">
                    {cert.serial}
                  </td>
                  <td className="px-6 py-4 whitespace-nowrap">
                    <div className="text-sm">
                      <p className="text-gray-900">
                        {new Date(cert.not_after).toLocaleDateString()}
                      </p>
                      <p
                        className={clsx(
                          'text-xs',
                          isExpired && 'text-red-600',
                          isExpiringSoon && !isExpired && 'text-orange-600',
                          !isExpired && !isExpiringSoon && 'text-gray-500'
                        )}
                      >
                        {isExpired
                          ? `Expired ${Math.abs(daysUntilExpiry)} days ago`
                          : `${daysUntilExpiry} days remaining`}
                      </p>
                    </div>
                  </td>
                  <td className="px-6 py-4 whitespace-nowrap">
                    {cert.state === 'signed' && !isExpired && (
                      <span className="inline-flex items-center gap-1 px-2 py-1 text-xs font-medium text-green-700 bg-green-100 rounded-full">
                        <CheckCircle2 className="w-3 h-3" /> Valid
                      </span>
                    )}
                    {cert.state === 'signed' && isExpired && (
                      <span className="inline-flex items-center gap-1 px-2 py-1 text-xs font-medium text-red-700 bg-red-100 rounded-full">
                        <XCircle className="w-3 h-3" /> Expired
                      </span>
                    )}
                    {cert.state === 'revoked' && (
                      <span className="inline-flex items-center gap-1 px-2 py-1 text-xs font-medium text-gray-700 bg-gray-100 rounded-full">
                        <FileX className="w-3 h-3" /> Revoked
                      </span>
                    )}
                  </td>
                  <td className="px-6 py-4 whitespace-nowrap text-right">
                    {cert.state === 'signed' && (
                      <button
                        onClick={() => onRevoke(cert.certname)}
                        className="inline-flex items-center gap-1 px-3 py-1.5 text-sm font-medium text-red-700 bg-red-100 rounded-md hover:bg-red-200"
                      >
                        <FileX className="w-4 h-4" />
                        Revoke
                      </button>
                    )}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>

      <p className="text-sm text-gray-500">
        Showing {filteredCerts.length} of {certificates.length} certificates
      </p>
    </div>
  );
}
