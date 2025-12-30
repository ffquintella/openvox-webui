# frozen_string_literal: true

# @summary Looks up a class parameter from OpenVox classification
#
# This function looks up parameters for a class from the openvox_classification
# fact. It's designed to be used in class parameter defaults or in conjunction
# with Hiera.
#
# The lookup follows this pattern:
# - For a key like 'profile::web_server::port', it looks for the 'port'
#   parameter of the 'profile::web_server' class in the classification.
#
# @param key The parameter key to lookup (class::name::param format)
# @param default Optional default value if not found
#
# @return The parameter value or default
#
# @example Lookup a class parameter
#   $port = openvox_webui::classification_lookup('profile::web_server::port')
#
# @example With default value
#   $port = openvox_webui::classification_lookup('profile::web_server::port', 8080)
#
Puppet::Functions.create_function(:'openvox_webui::classification_lookup') do
  dispatch :lookup_param do
    param 'String', :key
    optional_param 'Any', :default
    return_type 'Any'
  end

  def lookup_param(key, default = nil)
    # Parse the key to extract class name and parameter name
    # Format: class::name::parameter
    parts = key.split('::')

    if parts.length < 2
      Puppet.debug("OpenVox lookup: Invalid key format '#{key}', expected 'class::name::param'")
      return default
    end

    # Last part is the parameter name, rest is the class name
    param_name = parts.pop
    class_name = parts.join('::')

    # Get classification from facts
    classification = closure_scope['facts']['openvox_classification']

    if classification.nil?
      Puppet.debug("OpenVox lookup: No classification data available")
      return default
    end

    classes = classification['classes'] || {}

    # Look up the class
    class_params = classes[class_name]

    if class_params.nil?
      Puppet.debug("OpenVox lookup: Class #{class_name} not in classification")
      return default
    end

    unless class_params.is_a?(Hash)
      Puppet.debug("OpenVox lookup: Class #{class_name} has invalid params type")
      return default
    end

    # Look up the parameter
    if class_params.key?(param_name)
      Puppet.debug("OpenVox lookup: Found #{key} = #{class_params[param_name]}")
      class_params[param_name]
    else
      Puppet.debug("OpenVox lookup: Parameter #{param_name} not found for #{class_name}")
      default
    end
  end
end
