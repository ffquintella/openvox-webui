import { useState } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { User, Lock, Mail, AlertCircle, CheckCircle, Eye, EyeOff } from 'lucide-react';
import { api } from '../services/api';
import { useAuthStore } from '../stores/authStore';

export default function Profile() {
  const user = useAuthStore((state) => state.user);
  const queryClient = useQueryClient();

  const [currentPassword, setCurrentPassword] = useState('');
  const [newPassword, setNewPassword] = useState('');
  const [confirmPassword, setConfirmPassword] = useState('');
  const [showCurrentPassword, setShowCurrentPassword] = useState(false);
  const [showNewPassword, setShowNewPassword] = useState(false);
  const [showConfirmPassword, setShowConfirmPassword] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState<string | null>(null);

  const changePasswordMutation = useMutation({
    mutationFn: () => api.changePassword(currentPassword, newPassword),
    onSuccess: () => {
      setSuccess('Password changed successfully');
      setError(null);
      setCurrentPassword('');
      setNewPassword('');
      setConfirmPassword('');
      // Refresh user data to clear force_password_change flag
      queryClient.invalidateQueries({ queryKey: ['currentUser'] });
    },
    onError: (err: Error) => {
      setError(err.message || 'Failed to change password');
      setSuccess(null);
    },
  });

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    setSuccess(null);

    if (!currentPassword) {
      setError('Current password is required');
      return;
    }

    if (!newPassword) {
      setError('New password is required');
      return;
    }

    if (newPassword.length < 8) {
      setError('New password must be at least 8 characters');
      return;
    }

    if (newPassword !== confirmPassword) {
      setError('New passwords do not match');
      return;
    }

    if (currentPassword === newPassword) {
      setError('New password must be different from current password');
      return;
    }

    changePasswordMutation.mutate();
  };

  return (
    <div className="max-w-2xl mx-auto">
      <div className="mb-8">
        <h1 className="text-2xl font-bold text-gray-900">Profile</h1>
        <p className="text-gray-500 mt-1">Manage your account settings</p>
      </div>

      {/* User Info Card */}
      <div className="card mb-6">
        <h2 className="text-lg font-semibold text-gray-900 mb-4">Account Information</h2>
        <div className="space-y-4">
          <div className="flex items-center">
            <div className="w-16 h-16 rounded-full bg-primary-100 flex items-center justify-center mr-4">
              <User className="w-8 h-8 text-primary-600" />
            </div>
            <div>
              <p className="font-medium text-gray-900">{user?.username}</p>
              <div className="flex items-center text-gray-500 text-sm">
                <Mail className="w-4 h-4 mr-1" />
                {user?.email}
              </div>
            </div>
          </div>
          <div className="flex items-center gap-2">
            <span className="text-sm text-gray-500">Role:</span>
            <span className="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium bg-primary-100 text-primary-800 capitalize">
              {user?.role}
            </span>
          </div>
        </div>
      </div>

      {/* Change Password Card */}
      <div className="card">
        <h2 className="text-lg font-semibold text-gray-900 mb-4">Change Password</h2>

        {/* Success Message */}
        {success && (
          <div className="mb-4 p-4 bg-success-50 border border-success-200 rounded-lg flex items-start gap-3">
            <CheckCircle className="w-5 h-5 text-success-500 flex-shrink-0 mt-0.5" />
            <p className="text-sm text-success-700">{success}</p>
          </div>
        )}

        {/* Error Message */}
        {error && (
          <div className="mb-4 p-4 bg-danger-50 border border-danger-200 rounded-lg flex items-start gap-3">
            <AlertCircle className="w-5 h-5 text-danger-500 flex-shrink-0 mt-0.5" />
            <p className="text-sm text-danger-700">{error}</p>
          </div>
        )}

        <form onSubmit={handleSubmit} className="space-y-4">
          {/* Current Password */}
          <div>
            <label htmlFor="currentPassword" className="block text-sm font-medium text-gray-700 mb-1">
              Current Password
            </label>
            <div className="relative">
              <div className="absolute inset-y-0 left-0 pl-3 flex items-center pointer-events-none">
                <Lock className="h-5 w-5 text-gray-400" />
              </div>
              <input
                id="currentPassword"
                type={showCurrentPassword ? 'text' : 'password'}
                value={currentPassword}
                onChange={(e) => setCurrentPassword(e.target.value)}
                className="input pl-10 pr-10"
                placeholder="Enter current password"
              />
              <button
                type="button"
                onClick={() => setShowCurrentPassword(!showCurrentPassword)}
                className="absolute inset-y-0 right-0 pr-3 flex items-center text-gray-400 hover:text-gray-600"
              >
                {showCurrentPassword ? <EyeOff className="h-5 w-5" /> : <Eye className="h-5 w-5" />}
              </button>
            </div>
          </div>

          {/* New Password */}
          <div>
            <label htmlFor="newPassword" className="block text-sm font-medium text-gray-700 mb-1">
              New Password
            </label>
            <div className="relative">
              <div className="absolute inset-y-0 left-0 pl-3 flex items-center pointer-events-none">
                <Lock className="h-5 w-5 text-gray-400" />
              </div>
              <input
                id="newPassword"
                type={showNewPassword ? 'text' : 'password'}
                value={newPassword}
                onChange={(e) => setNewPassword(e.target.value)}
                className="input pl-10 pr-10"
                placeholder="Enter new password"
              />
              <button
                type="button"
                onClick={() => setShowNewPassword(!showNewPassword)}
                className="absolute inset-y-0 right-0 pr-3 flex items-center text-gray-400 hover:text-gray-600"
              >
                {showNewPassword ? <EyeOff className="h-5 w-5" /> : <Eye className="h-5 w-5" />}
              </button>
            </div>
            <p className="mt-1 text-xs text-gray-500">Must be at least 8 characters</p>
          </div>

          {/* Confirm Password */}
          <div>
            <label htmlFor="confirmPassword" className="block text-sm font-medium text-gray-700 mb-1">
              Confirm New Password
            </label>
            <div className="relative">
              <div className="absolute inset-y-0 left-0 pl-3 flex items-center pointer-events-none">
                <Lock className="h-5 w-5 text-gray-400" />
              </div>
              <input
                id="confirmPassword"
                type={showConfirmPassword ? 'text' : 'password'}
                value={confirmPassword}
                onChange={(e) => setConfirmPassword(e.target.value)}
                className="input pl-10 pr-10"
                placeholder="Confirm new password"
              />
              <button
                type="button"
                onClick={() => setShowConfirmPassword(!showConfirmPassword)}
                className="absolute inset-y-0 right-0 pr-3 flex items-center text-gray-400 hover:text-gray-600"
              >
                {showConfirmPassword ? <EyeOff className="h-5 w-5" /> : <Eye className="h-5 w-5" />}
              </button>
            </div>
          </div>

          {/* Submit Button */}
          <div className="pt-2">
            <button
              type="submit"
              disabled={changePasswordMutation.isPending}
              className="btn btn-primary"
            >
              {changePasswordMutation.isPending ? (
                <div className="flex items-center gap-2">
                  <div className="animate-spin rounded-full h-4 w-4 border-b-2 border-white" />
                  <span>Changing Password...</span>
                </div>
              ) : (
                'Change Password'
              )}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
