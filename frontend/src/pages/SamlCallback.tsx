import { useEffect, useState } from 'react';
import { useNavigate, useSearchParams } from 'react-router-dom';
import { Loader2, AlertCircle } from 'lucide-react';
import { useAuthStore } from '../stores/authStore';
import { usePermissionsStore } from '../stores/permissionsStore';

export default function SamlCallback() {
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const login = useAuthStore((state) => state.login);
  const fetchPermissions = usePermissionsStore((state) => state.fetchPermissions);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const accessToken = searchParams.get('access_token');
    const refreshToken = searchParams.get('refresh_token');
    const redirect = searchParams.get('redirect') || '/';
    const errorParam = searchParams.get('error');

    if (errorParam) {
      setError(decodeURIComponent(errorParam));
      return;
    }

    if (!accessToken) {
      setError('No authentication token received');
      return;
    }

    // Store refresh token
    if (refreshToken) {
      localStorage.setItem('refresh_token', refreshToken);
    }

    // Decode the JWT to get user info
    try {
      const payload = JSON.parse(atob(accessToken.split('.')[1]));
      login(
        {
          id: payload.sub,
          username: payload.username,
          email: payload.email,
          role: payload.roles?.[0] || 'viewer',
        },
        accessToken
      );
      fetchPermissions(payload.sub);
      navigate(redirect, { replace: true });
    } catch {
      setError('Failed to process authentication response');
    }
  }, [searchParams, login, fetchPermissions, navigate]);

  if (error) {
    return (
      <div className="min-h-screen flex items-center justify-center bg-gray-100">
        <div className="max-w-md w-full mx-4">
          <div className="bg-white rounded-lg shadow-lg p-8 text-center">
            <div className="inline-flex items-center justify-center w-16 h-16 rounded-full bg-danger-100 mb-4">
              <AlertCircle className="w-8 h-8 text-danger-600" />
            </div>
            <h1 className="text-xl font-bold text-gray-900 mb-2">Authentication Failed</h1>
            <p className="text-gray-600 mb-6">{error}</p>
            <button
              onClick={() => navigate('/login', { replace: true })}
              className="inline-flex items-center justify-center px-4 py-2 border border-transparent rounded-lg shadow-sm text-sm font-medium text-white bg-primary-600 hover:bg-primary-700 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-primary-500"
            >
              Return to Login
            </button>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="min-h-screen flex items-center justify-center bg-gray-100">
      <div className="max-w-md w-full mx-4">
        <div className="bg-white rounded-lg shadow-lg p-8 text-center">
          <Loader2 className="w-12 h-12 text-primary-600 animate-spin mx-auto mb-4" />
          <h1 className="text-xl font-bold text-gray-900 mb-2">Completing Sign In</h1>
          <p className="text-gray-600">Please wait while we complete your authentication...</p>
        </div>
      </div>
    </div>
  );
}
