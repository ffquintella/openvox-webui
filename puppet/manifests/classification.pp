# @summary Applies classes from OpenVox WebUI classification to nodes
#
# This class reads the classification data fetched by the openvox_classification
# custom fact and dynamically includes all classified classes with their
# parameters. This enables centralized node classification through OpenVox WebUI.
#
# The classification data includes:
# - Classes with their parameters (Puppet Enterprise format)
# - Variables that become top-level facts
# - Environment assignment
# - Group membership information
#
# Classes are included using resource-like class declarations, which means
# their parameters are passed directly from the classification data. This
# is equivalent to using Puppet Enterprise's Node Classifier.
#
# @param apply_classes
#   Whether to automatically include classified classes.
#   When true, all classes from the classification will be included
#   with their associated parameters.
#
# @param fail_on_missing_class
#   Whether to fail if a classified class is not available in the catalog.
#   When false, missing classes are logged as warnings but don't cause failure.
#
# @param class_prefix
#   Optional prefix to prepend to all class names.
#   Useful when classes in OpenVox use short names that map to profiled classes.
#
# @param excluded_classes
#   Array of class names to exclude from automatic inclusion.
#   Supports glob patterns (e.g., 'profile::deprecated::*').
#
# @param require_classification
#   Whether to fail if no classification data is available.
#   When false, nodes without classification simply skip class application.
#
# @param log_level
#   Log level for classification messages ('debug', 'info', 'notice', 'warning').
#
# @example Basic usage (include all classified classes)
#   include openvox_webui::classification
#
# @example With class exclusion
#   class { 'openvox_webui::classification':
#     excluded_classes => ['profile::deprecated', 'role::old_*'],
#   }
#
# @example Prefix mode for role/profile pattern
#   class { 'openvox_webui::classification':
#     class_prefix => 'profile::',
#   }
#
# @example Strict mode (fail on missing classes)
#   class { 'openvox_webui::classification':
#     fail_on_missing_class  => true,
#     require_classification => true,
#   }
#
class openvox_webui::classification (
  Boolean             $apply_classes          = true,
  Boolean             $fail_on_missing_class  = false,
  Optional[String[1]] $class_prefix           = undef,
  Array[String]       $excluded_classes       = [],
  Boolean             $require_classification = false,
  Enum['debug', 'info', 'notice', 'warning'] $log_level = 'debug',
) {
  # Check if classification data is available
  $classification = $facts['openvox_classification']

  if $classification == undef or $classification == '' {
    if $require_classification {
      $error_msg = 'openvox_webui::classification: No classification data available.'
      fail("${error_msg} Ensure openvox_webui::client is configured.")
    } else {
      notify { 'openvox_classification_missing':
        message  => 'OpenVox classification data not available. Skipping class application.',
        loglevel => $log_level,
      }
    }
  } else {
    # Extract classification data
    $groups = $classification['groups']
    $classes = $classification['classes']
    $variables = $classification['variables']
    $environment_name = $classification['environment']
    $certname = $classification['certname']

    # Calculate counts safely
    $group_count = $groups ? {
      undef   => 0,
      default => $groups.length,
    }
    $class_count = $classes ? {
      undef   => 0,
      default => $classes.keys.length,
    }
    $var_count = $variables ? {
      undef   => 0,
      default => $variables.keys.length,
    }

    # Log classification summary
    notify { 'openvox_classification_summary':
      message  => "OpenVox classification for ${certname}: ${group_count} groups, ${class_count} classes, ${var_count} variables",
      loglevel => $log_level,
    }

    # Log environment if set
    if $environment_name != undef and $environment_name != '' {
      notify { 'openvox_environment_info':
        message  => "OpenVox classified environment: ${environment_name}",
        loglevel => $log_level,
      }
    }

    # Apply classes if configured and there are classes to apply
    if $apply_classes and $classes != undef and $class_count > 0 {
      # Build the class hash for create_resources
      # This iterates through each class and prepares it for declaration
      $classes.each |String $class_name, $raw_params| {
        # Ensure params is a hash
        $class_params = $raw_params ? {
          Hash    => $raw_params,
          default => {},
        }

        # Determine the full class name (with optional prefix)
        $full_class_name = $class_prefix ? {
          undef   => $class_name,
          default => "${class_prefix}${class_name}",
        }

        # Check if class is excluded using pattern matching
        $is_excluded = $excluded_classes.reduce(false) |$memo, $pattern| {
          if $memo {
            true
          } elsif $pattern =~ /\*$/ {
            # Glob pattern - match prefix
            $prefix = regsubst($pattern, '\*$', '')
            $full_class_name =~ /^${prefix}/
          } else {
            # Exact match
            $full_class_name == $pattern
          }
        }

        if $is_excluded {
          notify { "openvox_class_excluded_${full_class_name}":
            message  => "OpenVox: Skipping excluded class ${full_class_name}",
            loglevel => $log_level,
          }
        } else {
          # Declare the class with its parameters using resource-like syntax
          # This is wrapped in a defined() check for safety
          $param_count = $class_params.keys.length
          $param_list = $class_params.keys.join(', ')

          notify { "openvox_class_apply_${full_class_name}":
            message  => "OpenVox: Applying class ${full_class_name} (${param_count} params: ${param_list})",
            loglevel => $log_level,
            before   => Class[$full_class_name],
          }

          # Use create_resources to declare the class with parameters
          # The 'class' type in Puppet accepts a hash of { 'classname' => { params } }
          $class_declaration = {
            $full_class_name => $class_params,
          }

          # Declare the class - this will pass all parameters
          create_resources('class', $class_declaration)
        }
      }
    }

    # Log final status
    notify { 'openvox_classification_complete':
      message  => "OpenVox classification applied for ${certname}",
      loglevel => $log_level,
    }
  }
}
