import { render, screen } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { ModelsTab } from './ModelsTab';
import type { ModelInfo } from '@/types';
const mockDeleteModel = vi.fn();
const mockUpdateSettings = vi.fn();
let capturedOnDelete: (name: string) => Promise<void> = () => Promise.resolve();
let capturedOnSelect: (name: string) => Promise<void> | void = () => Promise.resolve();
let mockSettings = {
  current_model: 'base.en',
  current_model_engine: 'whisper' as const,
  speech_language: 'ja',
};


// Mock sonner
vi.mock('sonner', () => ({
  toast: {
    info: vi.fn(),
    warning: vi.fn(),
    error: vi.fn(),
    success: vi.fn()
  }
}));

// Mock contexts
vi.mock('@/contexts/SettingsContext', () => ({
  useSettings: () => ({
    settings: mockSettings,
    updateSettings: mockUpdateSettings
  })
}));

// Mock hooks
let mockModels: Record<string, ModelInfo> = {
  'base.en': {
    name: 'base.en',
    display_name: 'Base English',
    size: 74,
    url: '',
    sha256: '',
    downloaded: true,
    speed_score: 7,
    accuracy_score: 5,
    recommended: false,
    engine: 'whisper',
    kind: 'local' as const,
    requires_setup: false
  },
  'small.en': {
    name: 'small.en',
    display_name: 'Small English',
    size: 244,
    url: '',
    sha256: '',
    downloaded: false,
    speed_score: 5,
    accuracy_score: 7,
    recommended: false,
    engine: 'whisper',
    kind: 'local' as const,
    requires_setup: false
  },
  'parakeet-unified-640ms': {
    name: 'parakeet-unified-640ms',
    display_name: 'Parakeet Unified (English)',
    size: 620,
    url: '',
    sha256: '',
    downloaded: true,
    speed_score: 9,
    accuracy_score: 10,
    recommended: true,
    engine: 'parakeet',
    kind: 'local',
    requires_setup: false,
    supported_languages: ['en']
  },
  'nemotron-multilingual-1120ms': {
    name: 'nemotron-multilingual-1120ms',
    display_name: 'Nemotron Multilingual',
    size: 665,
    url: '',
    sha256: '',
    downloaded: true,
    speed_score: 8,
    accuracy_score: 9,
    recommended: true,
    engine: 'parakeet',
    kind: 'local',
    requires_setup: false,
    supported_languages: ['en', 'ja', 'vi']
  },
};

// Mock the ModelManagementContext that ModelsTab actually imports
vi.mock('@/contexts/ModelManagementContext', () => ({
  useModelManagementContext: () => ({
    models: mockModels,
    downloadProgress: {},
    verifyingModels: new Set(),
    downloadPhases: {},
    downloadErrors: { 'small.en': 'Network error' },
    isLoading: true,
    sortedModels: Object.entries(mockModels),
    downloadModel: vi.fn(),
    deleteModel: mockDeleteModel,
    cancelDownload: vi.fn(),
    retryDownload: vi.fn(),
    refreshModels: vi.fn(),
    loadModels: vi.fn(),
    preloadModel: vi.fn(),
    verifyModel: vi.fn()
  })
}));

vi.mock('@/hooks/useEventCoordinator', () => ({
  useEventCoordinator: () => ({
    registerEvent: vi.fn((event: string, callback: any) => {
      (window as any).__testEventCallbacks = (window as any).__testEventCallbacks || {};
      (window as any).__testEventCallbacks[event] = callback;
      return vi.fn();
    })
  })
}));

// Mock ModelsSection component
vi.mock('@/components/sections/ModelsSection', () => ({
  ModelsSection: ({
    models,
    currentModel,
    downloadErrors,
    isLoading,
    onDelete,
    onSelect,
  }: {
    models: [string, ModelInfo][];
    currentModel?: string;
    downloadErrors?: Record<string, string>;
    isLoading?: boolean;
    onDelete: (name: string) => Promise<void>;
    onSelect: (name: string) => Promise<void> | void;
  }) => {
    capturedOnDelete = onDelete;
    capturedOnSelect = onSelect;
    return (
      <div data-testid="models-section">
        <div>Current Model: {currentModel}</div>
        <div>Models Count: {models.length}</div>
        <div>Small Error: {downloadErrors?.['small.en']}</div>
        <div>Loading: {String(isLoading)}</div>
      </div>
    );
  }
}));

describe('ModelsTab', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    (window as any).__testEventCallbacks = {};
    mockSettings = {
      current_model: 'base.en',
      current_model_engine: 'whisper',
      speech_language: 'ja',
    };
  });

  it('displays current model and available models', () => {
    render(<ModelsTab />);
    expect(screen.getByText('Current Model: base.en')).toBeInTheDocument();
    expect(screen.getByText('Models Count: 4')).toBeInTheDocument();
  });

  it('passes hook-owned errors and loading state to ModelsSection', () => {
    render(<ModelsTab />);

    expect(screen.getByText('Small Error: Network error')).toBeInTheDocument();
    expect(screen.getByText('Loading: true')).toBeInTheDocument();
  });

  it('swallows a deleteModel rejection without clearing the model selection', async () => {
    mockDeleteModel.mockRejectedValueOnce(new Error('delete failed'));

    render(<ModelsTab />);
    // mock ModelsSection mounted and captured the handler
    expect(screen.getByTestId('models-section')).toBeInTheDocument();

    // ModelCard calls onDelete(name) fire-and-forget; a failing delete_model
    // must not escape as an unhandled rejection, and selection stays unchanged.
    await expect(capturedOnDelete('base.en')).resolves.toBeUndefined();

    expect(mockDeleteModel).toHaveBeenCalledWith('base.en');
    expect(mockUpdateSettings).not.toHaveBeenCalled();
  });

  it('preserves a language supported by Nemotron', async () => {
    render(<ModelsTab />);

    await capturedOnSelect('nemotron-multilingual-1120ms');

    expect(mockUpdateSettings).toHaveBeenCalledWith({
      current_model: 'nemotron-multilingual-1120ms',
      current_model_engine: 'parakeet',
    });
  });

  it('resets the language for English-only Unified', async () => {
    render(<ModelsTab />);

    await capturedOnSelect('parakeet-unified-640ms');

    expect(mockUpdateSettings).toHaveBeenCalledWith({
      current_model: 'parakeet-unified-640ms',
      current_model_engine: 'parakeet',
      speech_language: 'en',
    });
  });

});
