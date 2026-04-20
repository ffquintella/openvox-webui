import { ShieldX, ArrowLeft } from 'lucide-react';
import { useNavigate } from 'react-router-dom';

interface AccessDeniedProps {
  message?: string;
  showBackButton?: boolean;
}

/**
 * Access denied page component shown when user lacks required permissions.
 */
export function AccessDenied({
  message = 'You do not have permission to access this page.',
  showBackButton = true,
}: AccessDeniedProps) {
  const navigate = useNavigate();

  return (
    <div className="flex flex-col items-center justify-center min-h-[400px] text-center">
      <div className="w-16 h-16 rounded-full bg-red-100 flex items-center justify-center mb-6">
        <ShieldX className="w-8 h-8 text-red-600" />
      </div>
      <h1 className="text-2xl font-bold text-gray-900 mb-2">Access Denied</h1>
      <p className="text-gray-600 mb-6 max-w-md">{message}</p>
      {showBackButton && (
        <button
          onClick={() => navigate(-1)}
          className="btn btn-secondary flex items-center"
        >
          <ArrowLeft className="w-4 h-4 mr-2" />
          Go Back
        </button>
      )}
    </div>
  );
}

interface InlineAccessDeniedProps {
  message?: string;
}

/**
 * Smaller inline version for use within cards/sections.
 */
export function InlineAccessDenied({
  message = 'Insufficient permissions',
}: InlineAccessDeniedProps) {
  return (
    <div className="flex items-center justify-center p-8 bg-gray-50 rounded-lg">
      <ShieldX className="w-5 h-5 text-gray-400 mr-2" />
      <span className="text-gray-500">{message}</span>
    </div>
  );
}
