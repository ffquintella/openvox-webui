import { Info, Github, Scale, ExternalLink } from 'lucide-react';

declare const __APP_VERSION__: string;

export default function About() {
  const currentYear = new Date().getFullYear();
  const copyrightYears = currentYear > 2024 ? `2024-${currentYear}` : '2024';

  return (
    <div className="max-w-2xl mx-auto">
      <div className="mb-8">
        <h1 className="text-2xl font-bold text-gray-900 dark:text-gray-100">About</h1>
        <p className="text-gray-500 dark:text-gray-400 mt-1">
          Information about OpenVox WebUI
        </p>
      </div>

      {/* Project Info Card */}
      <div className="card mb-6">
        <div className="flex items-center mb-4">
          <div className="w-12 h-12 rounded-lg bg-primary-100 dark:bg-primary-900/40 flex items-center justify-center mr-4">
            <Info className="w-6 h-6 text-primary-600 dark:text-primary-400" />
          </div>
          <div>
            <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-100">
              OpenVox WebUI
            </h2>
            <p className="text-sm text-gray-500 dark:text-gray-400">
              Version {__APP_VERSION__}
            </p>
          </div>
        </div>
        <p className="text-gray-600 dark:text-gray-300 mb-4">
          A modern web interface for managing OpenVox infrastructure. Provides PuppetDB integration,
          node classification, facter generation, and comprehensive dashboard visualization.
        </p>
        <div className="flex flex-wrap gap-2">
          <span className="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium bg-blue-100 text-blue-800 dark:bg-blue-900/40 dark:text-blue-300">
            Rust + Axum
          </span>
          <span className="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium bg-cyan-100 text-cyan-800 dark:bg-cyan-900/40 dark:text-cyan-300">
            React + TypeScript
          </span>
          <span className="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium bg-purple-100 text-purple-800 dark:bg-purple-900/40 dark:text-purple-300">
            Tailwind CSS
          </span>
          <span className="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium bg-orange-100 text-orange-800 dark:bg-orange-900/40 dark:text-orange-300">
            SQLite
          </span>
        </div>
      </div>

      {/* License Card */}
      <div className="card mb-6">
        <div className="flex items-center mb-4">
          <div className="w-12 h-12 rounded-lg bg-green-100 dark:bg-green-900/40 flex items-center justify-center mr-4">
            <Scale className="w-6 h-6 text-green-600 dark:text-green-400" />
          </div>
          <div>
            <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-100">License</h2>
            <p className="text-sm text-gray-500 dark:text-gray-400">Apache License 2.0</p>
          </div>
        </div>
        <p className="text-gray-600 dark:text-gray-300 mb-4">
          This project is licensed under the Apache License, Version 2.0. You may obtain a copy of
          the License at:
        </p>
        <a
          href="http://www.apache.org/licenses/LICENSE-2.0"
          target="_blank"
          rel="noopener noreferrer"
          className="inline-flex items-center gap-2 text-primary-600 hover:text-primary-700 dark:text-primary-400 dark:hover:text-primary-300 transition-colors"
        >
          <ExternalLink className="w-4 h-4" />
          http://www.apache.org/licenses/LICENSE-2.0
        </a>
        <p className="text-sm text-gray-500 dark:text-gray-400 mt-4">
          Unless required by applicable law or agreed to in writing, software distributed under
          the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
          KIND, either express or implied.
        </p>
      </div>

      {/* Links Card */}
      <div className="card mb-6">
        <div className="flex items-center mb-4">
          <div className="w-12 h-12 rounded-lg bg-gray-100 dark:bg-gray-700 flex items-center justify-center mr-4">
            <Github className="w-6 h-6 text-gray-600 dark:text-gray-400" />
          </div>
          <div>
            <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-100">Links</h2>
            <p className="text-sm text-gray-500 dark:text-gray-400">Project resources</p>
          </div>
        </div>
        <div className="space-y-3">
          <a
            href="https://github.com/ffquintella/openvox-webui"
            target="_blank"
            rel="noopener noreferrer"
            className="flex items-center gap-2 text-primary-600 hover:text-primary-700 dark:text-primary-400 dark:hover:text-primary-300 transition-colors"
          >
            <ExternalLink className="w-4 h-4" />
            GitHub Repository
          </a>
          <a
            href="https://voxpupuli.org/openvox/"
            target="_blank"
            rel="noopener noreferrer"
            className="flex items-center gap-2 text-primary-600 hover:text-primary-700 dark:text-primary-400 dark:hover:text-primary-300 transition-colors"
          >
            <ExternalLink className="w-4 h-4" />
            OpenVox Project
          </a>
        </div>
      </div>

      {/* Copyright Card */}
      <div className="card">
        <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-100 mb-4">Copyright</h2>
        <p className="text-gray-600 dark:text-gray-300">
          Copyright {copyrightYears} Felipe Quintella
        </p>
        <p className="text-sm text-gray-500 dark:text-gray-400 mt-2">
          All rights reserved under the terms of the Apache License 2.0.
        </p>
      </div>
    </div>
  );
}
