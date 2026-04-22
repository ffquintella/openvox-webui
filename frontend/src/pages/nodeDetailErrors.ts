type ApiErrorResponse = {
  response?: {
    status?: number;
    data?: {
      message?: string;
    };
  };
  message?: string;
};

export function getDeleteNodeErrorMessage(error: unknown): string {
  if (typeof error !== 'object' || error === null) {
    return 'Unknown error';
  }

  const apiError = error as ApiErrorResponse;
  if (apiError.response?.status === 403) {
    return 'You do not have authorization to delete nodes.';
  }

  return apiError.response?.data?.message || apiError.message || 'Unknown error';
}
