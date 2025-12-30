# frozen_string_literal: true

# @summary Declares a single class with parameters from OpenVox classification
#
# This function declares a class using resource-like syntax, which allows
# passing parameters directly. It reads the parameters from the
# openvox_classification fact.
#
# @param class_name The name of the class to declare
# @param additional_params Additional parameters to merge (overrides classification)
#
# @return [Boolean] True if class was declared successfully
#
# @example Declare a class from classification
#   openvox_webui::declare_classified_class('profile::web_server')
#
# @example With additional parameters
#   openvox_webui::declare_classified_class('profile::web_server', { 'extra_port' => 8443 })
#
Puppet::Functions.create_function(:'openvox_webui::declare_classified_class') do
  dispatch :declare_class do
    param 'String', :class_name
    optional_param 'Hash', :additional_params
    return_type 'Boolean'
  end

  def declare_class(class_name, additional_params = {})
    # Get classification from facts
    classification = closure_scope['facts']['openvox_classification']

    if classification.nil?
      Puppet.warning("OpenVox: No classification data available for declaring #{class_name}")
      return false
    end

    classes = classification['classes'] || {}

    # Get parameters for this class from classification
    class_params = classes[class_name] || {}
    class_params = class_params.is_a?(Hash) ? class_params : {}

    # Merge with additional parameters (additional takes precedence)
    final_params = class_params.merge(additional_params)

    begin
      # Create the class resource
      # This uses Puppet's resource API to declare the class
      scope = closure_scope
      klass = scope.find_hostclass(class_name)

      if klass.nil?
        Puppet.warning("OpenVox: Class #{class_name} not found")
        return false
      end

      # Create resource with parameters
      resource = Puppet::Resource.new('class', class_name)
      final_params.each do |key, value|
        resource[key] = value
      end

      # Add to catalog
      scope.catalog.add_resource(resource)

      Puppet.debug("OpenVox: Declared class #{class_name} with #{final_params.keys.length} parameters")
      true
    rescue StandardError => e
      Puppet.warning("OpenVox: Failed to declare class #{class_name}: #{e.message}")
      false
    end
  end
end
